use tauri::{Manager, PhysicalPosition, PhysicalRect, PhysicalSize};

const ROAM_INITIAL_DX: i32 = 2;
const ROAM_INITIAL_DY: i32 = 1;
const ROAM_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);
const ROAM_VERIFY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);
const ROAM_STALL_LIMIT: u8 = 3;

const UNEXPECTED_MOVE_TOLERANCE: i64 = 32;

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

fn roam_tasks() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static TASKS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    TASKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_task(label: &str) -> Option<Arc<AtomicBool>> {
    let mut tasks = roam_tasks().lock().ok()?;
    let active = Arc::new(AtomicBool::new(true));
    if let Some(previous) = tasks.insert(label.to_owned(), active.clone()) {
        previous.store(false, Ordering::Release);
    }
    Some(active)
}

fn finish_task(label: &str, active: &Arc<AtomicBool>) {
    if let Ok(mut tasks) = roam_tasks().lock() {
        if tasks
            .get(label)
            .is_some_and(|current| Arc::ptr_eq(current, active))
        {
            tasks.remove(label);
        }
    }
}

pub(crate) fn spawn_for_pet(app: tauri::AppHandle, label: String) {
    if !crate::roam_enabled() {
        return;
    }
    let Some(active) = register_task(&label) else {
        eprintln!("desktop roaming unavailable: task registry poisoned");
        return;
    };
    tauri::async_runtime::spawn(async move {
        roam_desktop(app, &label, &active).await;
        finish_task(&label, &active);
    });
}

fn unexpected_displacement(
    requested: PhysicalPosition<i32>,
    observed: PhysicalPosition<i32>,
) -> bool {
    let dx = (i64::from(observed.x) - i64::from(requested.x)).abs();
    let dy = (i64::from(observed.y) - i64::from(requested.y)).abs();
    dx > UNEXPECTED_MOVE_TOLERANCE || dy > UNEXPECTED_MOVE_TOLERANCE
}

// 計算下一個受限位置
fn bounded_next_position(
    before: PhysicalPosition<i32>,
    window_size: PhysicalSize<u32>,
    work_area: &PhysicalRect<i32, u32>,
    dx: &mut i32,
    dy: &mut i32,
) -> PhysicalPosition<i32> {
    let min_x = i64::from(work_area.position.x);
    let min_y = i64::from(work_area.position.y);

    let area_width = i64::from(work_area.size.width);
    let area_height = i64::from(work_area.size.height);
    let window_width = i64::from(window_size.width);
    let window_height = i64::from(window_size.height);

    let max_x = min_x + area_width - window_width;
    let max_y = min_y + area_height - window_height;

    let mut next_x = i64::from(before.x) + i64::from(*dx);
    let mut next_y = i64::from(before.y) + i64::from(*dy);

    if max_x <= min_x {
        next_x = min_x;
        *dx = 0;
    } else if next_x < min_x {
        next_x = min_x;
        *dx = dx.saturating_abs();
    } else if next_x > max_x {
        next_x = max_x;
        *dx = -dx.saturating_abs();
    }

    if max_y <= min_y {
        next_y = min_y;
        *dy = 0;
    } else if next_y < min_y {
        next_y = min_y;
        *dy = dy.saturating_abs();
    } else if next_y > max_y {
        next_y = max_y;
        *dy = -dy.saturating_abs();
    }

    PhysicalPosition {
        x: next_x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        y: next_y.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
    }
}

fn no_roaming_space(dx: i32, dy: i32) -> bool {
    dx == 0 && dy == 0
}

async fn roam_desktop(app: tauri::AppHandle, label: &str, active: &AtomicBool) {
    let mut dx = ROAM_INITIAL_DX;
    let mut dy = ROAM_INITIAL_DY;
    let mut stalled_attempts = 0_u8;

    loop {
        if !active.load(Ordering::Acquire) {
            break;
        }

        let window = match app.get_webview_window(label) {
            Some(window) => window,
            None => {
                eprintln!("desktop roaming unavailable: pet window not found: {label}");
                break;
            }
        };

        let before = match window.outer_position() {
            Ok(position) => position,
            Err(error) => {
                eprintln!("desktop roaming unavailable: {error}");
                break;
            }
        };

        let window_size = match window.outer_size() {
            Ok(size) => size,
            Err(error) => {
                eprintln!("desktop roaming unavailable: {error}");
                break;
            }
        };

        let monitor = match window.current_monitor() {
            Ok(Some(monitor)) => monitor,
            Ok(None) => {
                eprintln!("desktop roaming unavailable: no monitor found");
                break;
            }
            Err(error) => {
                eprintln!("desktop roaming unavailable: {error}");
                break;
            }
        };

        let requested =
            bounded_next_position(before, window_size, monitor.work_area(), &mut dx, &mut dy);
        // 取消後不可再送出新的位移；不持有 registry Mutex 跨 await。
        if !active.load(Ordering::Acquire) {
            break;
        }
        if let Err(error) = window.set_position(requested) {
            eprintln!("desktop roaming unavailable: {error}");
            break;
        }
        if no_roaming_space(dx, dy) {
            eprintln!("desktop roaming stopped: pet window exceeds available work area");
            break;
        }
        tokio::time::sleep(ROAM_VERIFY_INTERVAL).await;
        if !active.load(Ordering::Acquire) {
            break;
        }

        let after = match window.outer_position() {
            Ok(position) => position,
            Err(error) => {
                eprintln!("desktop roaming verification unavailable: {error}");
                break;
            }
        };
        if unexpected_displacement(requested, after) {
            eprintln!("desktop roaming stopped: unexpected window displacement");
            break;
        }
        if after == before {
            stalled_attempts += 1;
            if stalled_attempts >= ROAM_STALL_LIMIT {
                eprintln!(
                    "desktop roaming unavailable: window position did not change after                      {ROAM_STALL_LIMIT} attempts"
                );
                break;
            }
        } else {
            stalled_attempts = 0;
        }

        tokio::time::sleep(ROAM_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day19_displacement_threshold_axes_and_extremes() {
        let p = |x, y| PhysicalPosition { x, y };
        assert!(!unexpected_displacement(p(-100, -100), p(-68, -132)));
        assert!(unexpected_displacement(p(-100, -100), p(-67, -100)));
        assert!(unexpected_displacement(p(-100, -100), p(-100, -133)));
        assert!(!unexpected_displacement(p(0, 0), p(23, 23)));
        assert!(unexpected_displacement(p(i32::MIN, 0), p(i32::MAX, 0)));
        assert!(unexpected_displacement(p(0, i32::MAX), p(0, i32::MIN)));
    }

    #[test]
    fn day19_large_external_reposition_stops_roam() {
        assert!(unexpected_displacement(
            PhysicalPosition { x: 100, y: 100 },
            PhysicalPosition { x: 400, y: 100 },
        ));
        assert!(!unexpected_displacement(
            PhysicalPosition { x: 100, y: 100 },
            PhysicalPosition { x: 102, y: 99 },
        ));
    }

    fn area(x: i32, y: i32, width: u32, height: u32) -> PhysicalRect<i32, u32> {
        PhysicalRect {
            position: PhysicalPosition { x, y },
            size: PhysicalSize { width, height },
        }
    }

    #[test]
    fn moves_inside_work_area() {
        let mut dx = 2;
        let mut dy = 1;
        let next = bounded_next_position(
            PhysicalPosition { x: 100, y: 100 },
            PhysicalSize {
                width: 320,
                height: 360,
            },
            &area(0, 0, 1920, 1080),
            &mut dx,
            &mut dy,
        );
        assert_eq!(next, PhysicalPosition { x: 102, y: 101 });
        assert_eq!((dx, dy), (2, 1));
    }

    #[test]
    fn bounces_at_right_and_bottom_edges() {
        let mut dx = 2;
        let mut dy = 1;
        let next = bounded_next_position(
            PhysicalPosition { x: 1599, y: 719 },
            PhysicalSize {
                width: 320,
                height: 360,
            },
            &area(0, 0, 1920, 1080),
            &mut dx,
            &mut dy,
        );
        assert_eq!(next, PhysicalPosition { x: 1600, y: 720 });
        assert_eq!((dx, dy), (-2, 1));
    }

    #[test]
    fn supports_negative_monitor_origin() {
        let mut dx = -2;
        let mut dy = -1;
        let next = bounded_next_position(
            PhysicalPosition { x: -1600, y: 0 },
            PhysicalSize {
                width: 320,
                height: 360,
            },
            &area(-1920, 0, 1920, 1080),
            &mut dx,
            &mut dy,
        );
        assert_eq!(next, PhysicalPosition { x: -1602, y: 0 });
        assert_eq!((dx, dy), (-2, 1));
    }

    #[test]
    fn oversized_window_is_pinned_without_panicking() {
        let mut dx = 2;
        let mut dy = 1;
        let next = bounded_next_position(
            PhysicalPosition { x: 10, y: 20 },
            PhysicalSize {
                width: 1000,
                height: 900,
            },
            &area(-100, -50, 800, 600),
            &mut dx,
            &mut dy,
        );
        assert_eq!(next, PhysicalPosition { x: -100, y: -50 });
        assert_eq!((dx, dy), (0, 0));
    }

    #[test]
    fn day38_window_label_is_per_pet_not_control_or_fixed_pet() {
        let id = uuid::Uuid::new_v4().to_string();
        let label = crate::pet_windows::label(&id).unwrap();
        assert_eq!(
            label,
            format!("pet-{}", uuid::Uuid::parse_str(&id).unwrap().simple())
        );
        assert_ne!(label, "pet");
        assert_ne!(label, "main");
    }

    #[test]
    fn day38_reopen_cancels_only_previous_task_for_same_label() {
        use std::sync::atomic::Ordering;
        let a = format!("pet-{}", uuid::Uuid::new_v4().simple());
        let b = format!("pet-{}", uuid::Uuid::new_v4().simple());
        let first = register_task(&a).unwrap();
        let other = register_task(&b).unwrap();
        let second = register_task(&a).unwrap();
        assert!(!first.load(Ordering::Acquire));
        assert!(second.load(Ordering::Acquire));
        assert!(other.load(Ordering::Acquire));
        finish_task(&a, &first);
        assert!(roam_tasks().lock().unwrap().contains_key(&a));
        finish_task(&a, &second);
        finish_task(&b, &other);
        assert!(!roam_tasks().lock().unwrap().contains_key(&a));
        assert!(!roam_tasks().lock().unwrap().contains_key(&b));
    }

    #[test]
    fn day20_stops_only_when_both_axes_are_blocked() {
        assert!(super::no_roaming_space(0, 0));
        assert!(!super::no_roaming_space(0, 1));
        assert!(!super::no_roaming_space(2, 0));
        assert!(!super::no_roaming_space(-1, 2));
    }

    #[test]
    fn day20_oversized_window_is_pinned_and_stops() {
        let mut dx = 2;
        let mut dy = 1;
        let next = super::bounded_next_position(
            PhysicalPosition { x: 20, y: 50 },
            PhysicalSize {
                width: 900,
                height: 1000,
            },
            &area(-100, -50, 800, 600),
            &mut dx,
            &mut dy,
        );
        assert_eq!(next, PhysicalPosition { x: -100, y: -50 });
        assert!(super::no_roaming_space(dx, dy));
    }

    #[test]
    fn day20_single_axis_can_still_roam() {
        let mut dx = 2;
        let mut dy = 1;
        let next = super::bounded_next_position(
            PhysicalPosition { x: 10, y: 10 },
            PhysicalSize {
                width: 900,
                height: 100,
            },
            &area(0, 0, 800, 600),
            &mut dx,
            &mut dy,
        );
        assert_eq!(next.x, 0);
        assert_eq!(next.y, 11);
        assert_eq!(dx, 0);
        assert_eq!(dy, 1);
        assert!(!super::no_roaming_space(dx, dy));
    }
}
