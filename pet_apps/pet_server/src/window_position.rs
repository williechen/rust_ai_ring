use tauri::{PhysicalPosition, PhysicalRect, PhysicalSize};

/// Pure native-window geometry. The caller owns show/focus and roam pause/resume.
pub(crate) fn centered_position(
    area: &PhysicalRect<i32, u32>,
    size: PhysicalSize<u32>,
) -> PhysicalPosition<i32> {
    let x = i64::from(area.position.x) + i64::from(area.size.width.saturating_sub(size.width) / 2);
    let y =
        i64::from(area.position.y) + i64::from(area.size.height.saturating_sub(size.height) / 2);
    PhysicalPosition {
        x: x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        y: y.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_work_area_never_places_window_before_its_origin() {
        let area = PhysicalRect {
            position: PhysicalPosition { x: -300, y: 20 },
            size: PhysicalSize {
                width: 100,
                height: 100,
            },
        };
        assert_eq!(
            centered_position(
                &area,
                PhysicalSize {
                    width: 200,
                    height: 300
                }
            ),
            PhysicalPosition { x: -300, y: 20 }
        );
        assert_eq!(
            centered_position(
                &area,
                PhysicalSize {
                    width: 20,
                    height: 20
                }
            ),
            PhysicalPosition { x: -260, y: 60 }
        );
    }
}
