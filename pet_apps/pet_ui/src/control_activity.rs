use crate::bridge;
use crate::{control_navigation::ControlPage, saved_pet::SavedPet};
use leptos::{prelude::*, task::spawn_local};
use pet_application::activity_control::{ActivityAction, execute};

#[component]
pub(crate) fn ActivityWorkspace(
    id: String,
    pet: Signal<Option<SavedPet>>,
    busy: ReadSignal<bool>,
    set_busy: WriteSignal<bool>,
    page: RwSignal<ControlPage>,
) -> impl IntoView {
    use ActivityAction::*;

    let natural_id = id.clone();
    let status = ArcRwSignal::new(None::<bridge::ActivityStatus>);
    let error = ArcRwSignal::new(None::<String>);

    let action = Callback::new({
        let (status, error) = (status.clone(), error.clone());
        move |choice: ActivityAction| {
            if busy.get_untracked() {
                return;
            }
            set_busy.set(true);
            error.set(None);
            let id = id.clone();
            let (status, error) = (status.clone(), error.clone());
            spawn_local(async move {
                let result = execute(&bridge::ActivityBridge, &id, choice).await;
                match result {
                    Ok(outcome) => match outcome.status {
                        Ok(next) => {
                            status.set(Some(next));
                        }
                        Err(message) => {
                            status.set(None);
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
                Some(state) if state.outside => "已出籠",
                Some(_) => "已回籠",
                None if busy.get() => "正在讀取狀態…",
                None => "目前狀態未確認",
            })
        }
    };

    let outside_action = is_outside.clone();

    view! {
        <div class="pet-workspace">
            <div class="pet-workspace-bar"
                role="group"
                aria-label="生活窗操作">
                <span class="pet-status-chip" role="status">{caption}</span>
                <button class="control-primary"
                    type="button"
                    disabled=ready.clone()
                    on:click=move |_| action.run(if outside_action() { Cage } else { Open })>
                    {move || if is_outside() { "回籠" } else { "出籠" }}
                </button>
                <button type="button"
                    data-action="refresh"
                    disabled=move || busy.get()
                    on:click=move |_| action.run(Refresh)
                >"更新活動"</button>
                <button type="button"
                    disabled=ready.clone()
                    on:click=move |_| action.run(Find)
                >"找回生活窗"</button>
            </div>
            <crate::pml::pml2001::web::NaturalActivity id=natural_id/>
            {move || error.get().map(|message| view! { <p class="control-error" role="alert">{message}</p> })}
            <div class="pet-workspace-columns">
                <div class="pet-workspace-side">
                    <crate::pml::pml2001::web::IdleChoices status=status.clone() busy action />
                    <section class="pet-settings-prompt">
                        <h3>"調整寵物資料"</h3>
                        <p>"名字、喜好和啟動預設集中在設定。"</p>
                        <button type="button"
                            on:click=move |_| {
                                page.set(ControlPage::Settings);
                                ControlPage::Settings.focus_tab();
                            }
                        >"前往設定"</button>
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
        <p role="status">
            {move || {
            if busy.get() {
                "正在讀取或更新啟動預設…".to_owned()
            } else if let Some(message) = error.get() {
                message
            } else if let Some((_, caged)) = current.get() {
                if caged { "啟動時留籠" } else { "啟動時出籠" }.to_owned()
            } else {
                "請先選擇寵物。".to_owned()
            }
        }}</p>
        <div class="control-activity-actions">
            <button type="button"
                disabled=move || busy.get() || selected.with(Option::is_none)
                on:click=move |_| request(3)>"啟動時出籠"</button>
            <button type="button"
                disabled=move || busy.get() || selected.with(Option::is_none)
                on:click=move |_| request(4)>"啟動時留籠"</button>
        </div>
    }
}
