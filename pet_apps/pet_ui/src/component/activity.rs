use gloo_timers::callback::{Interval, Timeout};
use leptos::prelude::*;
use pet_domain::{PetSnapshot, PetState};
use wasm_bindgen_futures::spawn_local;

use crate::{bridge, component::frames::frame_uri};

use super::pose::{PetPose, project_pose};

// 拖曳桌面小寵物的控制把手組件
#[component]
pub fn DragHandle() -> impl IntoView {
    view! {
        <header
            class="drag-handle"
            data-tauri-drag-region
            aria-label="拖曳桌面小寵物"
        >
            <span
                class="drag-grip"
                data-tauri-drag-region
                aria-hidden="true"
            >
                "..."
            </span>
        </header>
    }
}

#[component]
pub fn AnimationClock(set_animation_phase: WriteSignal<bool>) -> impl IntoView {
    let interval = Interval::new(650, move || {
        set_animation_phase.update(|phase| *phase = !*phase);
    });
    let _interval = StoredValue::new_local(interval);

    ().into_any()
}

// 控制桌面小寵物眨眼的組件
#[component]
pub fn BlinkController(
    snapshot: ReadSignal<Option<PetSnapshot>>,
    set_blinking: WriteSignal<bool>,
) -> impl IntoView {
    let interval = Interval::new(4_000, move || {
        let is_idle = snapshot
            .get_untracked()
            .map(|snapshot| snapshot.state == PetState::Idle)
            .unwrap_or(false);

        if !is_idle {
            set_blinking.set(false);
            return;
        }

        set_blinking.set(true);

        Timeout::new(180, move || {
            set_blinking.set(false);
        })
        .forget();
    });

    // gloo_timers::Interval 是 !Send / !Sync。
    // StoredValue::new_local() 會把它保存在目前 Leptos owner 的 local arena，
    // owner dispose 時一起 drop，Interval 也會因此取消。
    let _interval = StoredValue::new_local(interval);

    ().into_any()
}

// 顯示桌面小寵物狀態的組件
#[component]
pub fn PetStatus(
    snapshot: ReadSignal<Option<PetSnapshot>>,
    blinking: ReadSignal<bool>,
    animation_phase: ReadSignal<bool>,
) -> impl IntoView {
    let pose = move || {
        snapshot
            .get()
            .map(|s| project_pose(s.state, blinking.get(), animation_phase.get()))
    };
    let label = move || match pose() {
        Some(PetPose::Idle) => "待機",
        Some(PetPose::Blink) => "眨眼",
        Some(PetPose::InteractA | PetPose::InteractB) => "互動",
        Some(PetPose::SleepA | PetPose::SleepB) => "睡覺",
        None => "載入中",
    };
    let revision = move || {
        snapshot
            .get()
            .map(|s| s.revision.to_string())
            .unwrap_or_else(|| "-".into())
    };
    view! {
        <section class="pet-status">
            <div class="pet-face"
                on:contextmenu=move |event| {
                    event.prevent_default();
                    let (x,y) = (event.client_x(),event.client_y());
                    spawn_local(async move {
                        if let Err(error) = bridge::show_pet_context_menu(x,y).await {
                            web_sys::console::warn_1(&error.into());
                        }
                    });
                }>
                <img class=move || pose().map(PetPose::css_class)
                          .unwrap_or("pose-idle")
                    alt="寵物動畫"
                    src=move || frame_uri(pose().unwrap_or(PetPose::Idle))
                />
            </div>
            <strong>{label}</strong>
            <small>"revision "{revision}</small>
        </section>
    }
}

#[cfg(test)]
mod frame_render_tests {
    use super::*;
    fn render(state: PetState) -> String {
        Owner::new().with(|| {
            let (snapshot, _) = signal(Some(PetSnapshot { state, revision: 7 }));
            let (blinking, _) = signal(false);
            let (animation_phase, _) = signal(false);
            view! { <PetStatus snapshot blinking animation_phase/> }.to_html()
        })
    }
    #[test]
    fn actual_pet_status_serializes_one_state_specific_image() {
        let idle = render(PetState::Idle);
        let sleeping = render(PetState::Sleeping);
        for html in [&idle, &sleeping] {
            assert_eq!(html.matches("<img ").count(), 1);
            assert!(html.contains("alt=\"寵物動畫\""));
            assert!(html.contains("data:image/svg+xml,"));
            assert!(!html.contains("🐢"));
        }
        let src = |html: &str| {
            html.split("src=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_owned()
        };
        assert_ne!(src(&idle), src(&sleeping));
    }
}
