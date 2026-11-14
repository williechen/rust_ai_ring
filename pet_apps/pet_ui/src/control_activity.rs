use crate::{bridge, component::frames::frame_uri, component::pose::PetPose};
use crate::{control_navigation::ControlPage, saved_pet::SavedPet};
use leptos::{prelude::*, task::spawn_local};
use pet_application::activity_control::{ActivityAction, execute};
use pet_domain::PetState;

#[component]
pub(crate) fn ActivityWorkspace(
    id: String,
    pet: Signal<Option<SavedPet>>,
    busy: ReadSignal<bool>,
    set_busy: WriteSignal<bool>,
    page: RwSignal<ControlPage>,
) -> impl IntoView {
    use ActivityAction::*;
    let status = ArcRwSignal::new(None::<bridge::ActivityStatus>);
    let pose = ArcRwSignal::new(None::<PetState>);
    let error = ArcRwSignal::new(None::<String>);
    let action = Callback::new({
        let (status, pose, error) = (status.clone(), pose.clone(), error.clone());
        move |choice: ActivityAction| {
            if busy.get_untracked() {
                return;
            }
            set_busy.set(true);
            error.set(None);
            let id = id.clone();
            let (status, pose, error) = (status.clone(), pose.clone(), error.clone());
            spawn_local(async move {
                let result = execute(&bridge::ActivityBridge, &id, choice).await;
                match result {
                    Ok(outcome) => match outcome.status {
                        Ok(next) => {
                            let next_pose = if next.outside {
                                match bridge::get_pet_state_for(Some(id)).await {
                                    Ok(state) => Some(state.state),
                                    Err(message) => {
                                        error.set(Some(format!(
                                            "活動已讀取，姿態讀取失敗：{message}；請更新活動。"
                                        )));
                                        None
                                    }
                                }
                            } else {
                                None
                            };
                            status.set(Some(next));
                            pose.set(next_pose);
                        }
                        Err(message) => {
                            status.set(None);
                            pose.set(None);
                            let prefix = if outcome.command_confirmed {
                                "操作已回報成功，狀態讀取失敗"
                            } else {
                                "狀態讀取失敗"
                            };
                            error.set(Some(format!(
                                "{prefix}：{message}；請按更新活動確認，勿重送。"
                            )));
                        }
                    },
                    Err(message) => {
                        status.set(None);
                        pose.set(None);
                        error.set(Some(format!(
                            "操作結果未確認：{message}；請先更新活動，勿直接重送。"
                        )));
                    }
                }
                set_busy.set(false);
            });
        }
    });
    Effect::new(move |_| action.run(Refresh));

    let ready = {
        let status = status.clone();
        move || busy.get() || status.with(Option::is_none)
    };
    let is_outside = {
        let status = status.clone();
        move || status.with(|state| state.as_ref().is_some_and(|state| state.outside))
    };
    let caption = {
        let status = status.clone();
        move || {
            status.with(|value| match value {
                Some(state) if state.roaming => format!(
                    "已出籠 · {}",
                    state.walk_label.as_deref().unwrap_or("漫遊中")
                ),
                Some(state) if state.sitting => format!(
                    "已出籠 · {}",
                    state.sit_label.as_deref().unwrap_or("未漫遊")
                ),
                Some(state) if state.outside => "已出籠".into(),
                Some(_) => "已回籠".into(),
                None if busy.get() => "正在讀取狀態…".into(),
                None => "目前狀態未確認".into(),
            })
        }
    };
    let active_walk = {
        let status = status.clone();
        move || {
            status
                .with(|state| state.as_ref().is_some_and(|s| s.roaming))
                .to_string()
        }
    };
    let active_sit = {
        let status = status.clone();
        move || {
            status
                .with(|state| state.as_ref().is_some_and(|s| s.sitting))
                .to_string()
        }
    };
    let walk_label = {
        let status = status.clone();
        move || {
            status
                .with(|state| state.as_ref().and_then(|s| s.walk_label.clone()))
                .unwrap_or_else(|| "開始漫遊".into())
        }
    };
    let sit_label = move || {
        status
            .with(|state| state.as_ref().and_then(|s| s.sit_label.clone()))
            .unwrap_or_else(|| "停止漫遊".into())
    };
    let frame = pose.clone();
    let sleep_label = pose.clone();
    let sleep_ready = pose.clone();
    let click_sleep = pose.clone();
    let outside_action = is_outside.clone();
    view! {
       <div class="pet-workspace">
            <div class="pet-workspace-bar">
                <span class="pet-status-chip" role="status">{caption}</span>
                <button class="control-primary" type="button" disabled=ready.clone()
                    on:click=move |_| action.run(if outside_action() { Cage } else { Open })>
                    {move || if is_outside() { "回籠" } else { "出籠" }}
                </button>
                <details class="pet-workspace-more">
                    <summary>"更多"</summary>
                    <button type="button" disabled=move || busy.get() on:click=move |_| action.run(Refresh)>"更新活動"</button>
                    <button type="button" disabled=ready.clone() on:click=move |_| action.run(Find)>"找回生活窗"</button>
                </details>
            </div>
            {move || error.get().map(|message| view! { <p class="control-error" role="alert">{message}</p> })}
            <div class="pet-workspace-columns">
                <figure class="pet-preview">
                    <figcaption>"生活窗預覽"</figcaption>
                    <div class="pet-preview-image">
                        <Show when=move || pet.with(|row| row.as_ref().is_some_and(|row| row.species == "tortoise"))
                            fallback=|| view! { <p>"這個種類目前沒有預覽。"</p> }>
                            <img src={
                                let frame = frame.clone();
                                move || frame_uri(if frame.get() == Some(PetState::Sleeping) { PetPose::SleepA } else { PetPose::Idle })
                            } alt=""/>
                        </Show>
                    </div>
                    <strong>{move || pet.with(|row| row.as_ref().map(|pet| pet.name.clone()).unwrap_or_default())}</strong>
                    <p>"預覽不會建立另一個生活窗。"</p>
                </figure>
                <div class="pet-workspace-side">
                    <section class="control-activity" aria-label="所選寵物活動">
                        <h3>"目前活動"</h3>
                        <p>"選擇這隻寵物的活動。"</p>
                        <div class="control-activity-actions" role="group" aria-label="活動模式">
                            <button type="button" aria-pressed=active_sit disabled=ready.clone() on:click=move |_| action.run(Sit)>{sit_label}</button>
                            <button type="button" aria-pressed=active_walk disabled=ready.clone() on:click=move |_| action.run(Walk)>{walk_label}</button>
                        </div>
                        <button type="button" class="pet-sleep" disabled=move || busy.get() || sleep_ready.with(Option::is_none)
                            on:click=move |_| action.run(if click_sleep.get_untracked() == Some(PetState::Sleeping) { Wake } else { Sleep })>
                            {move || if sleep_label.get() == Some(PetState::Sleeping) { "醒來" } else { "睡覺" }}
                        </button>
                    </section>
                    <section class="pet-settings-prompt">
                        <h3>"調整寵物資料"</h3>
                        <p>"名字、喜好和啟動預設集中在設定。"</p>
                        <button type="button" on:click=move |_| {
                            page.set(ControlPage::Settings);
                            ControlPage::Settings.focus_tab();
                        }>"前往設定"</button>
                    </section>
                </div>
            </div>
        </div>
    }
}

#[component]
pub(crate) fn StartupPreferences(selected: ReadSignal<Option<String>>) -> impl IntoView {
    let (current, set_current) = signal(None::<(String, bool)>);
    let (error, set_error) = signal(None::<String>);
    let (busy, set_busy) = signal(false);
    let generation = RwSignal::new(0_u64);

    let request = move |choice| {
        let Some(id) = selected.get_untracked() else {
            return;
        };
        set_error.set(None);
        set_busy.set(true);
        let request_id = generation.get_untracked().wrapping_add(1);
        generation.set(request_id);
        spawn_local(async move {
            let result = bridge::activity_action(&id, choice).await;
            if generation.get_untracked() != request_id
                || selected.get_untracked().as_deref() != Some(&id)
            {
                return;
            }
            match result {
                Ok(status) => set_current.set(Some((id, status.default_caged))),
                Err(message) => set_error.set(Some(message)),
            }
            set_busy.set(false);
        });
    };
    Effect::new(move |_| {
        let id = selected.get();
        generation.update(|current| *current = current.wrapping_add(1));
        set_current.set(None);
        set_busy.set(false);
        set_error.set(None);
        if id.is_some() {
            request(0);
        }
    });

    view! {
        <p role="status">{move || {
            if busy.get() { "正在讀取或更新啟動預設…".to_owned() }
            else if let Some(message) = error.get() { message }
            else if let Some((_, caged)) = current.get() {
                if caged { "啟動時留籠" } else { "啟動時出籠" }.to_owned()
            } else { "請先選擇寵物。".to_owned() }
        }}</p>
        <div class="control-activity-actions">
            <button type="button" disabled=move || busy.get() || selected.with(Option::is_none)
                on:click=move |_| request(3)>"啟動時出籠"</button>
            <button type="button" disabled=move || busy.get() || selected.with(Option::is_none)
                on:click=move |_| request(4)>"啟動時留籠"</button>
        </div>
    }
}
