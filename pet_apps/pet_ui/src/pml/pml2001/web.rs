use leptos::prelude::*;
use pet_application::idle_dto::{IdleAction, IdleStatus};

#[component]
pub(crate) fn NaturalActivity(id: String) -> impl IntoView {
    let current = RwSignal::new(None::<IdleStatus>);
    let error = RwSignal::new(false);
    let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let guard = alive.clone();

    on_cleanup(move || guard.store(false, std::sync::atomic::Ordering::Relaxed));

    let load = {
        let id = id.clone();
        move || {
            let id = id.clone();
            let alive = alive.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = super::web_service::read(&id).await;
                if !alive.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                error.set(result.is_err());
                if let Ok(status) = result {
                    current.set(Some(status));
                }
            });
        }
    };

    load();

    let interval = gloo_timers::callback::Interval::new(1000, load);

    let _interval = StoredValue::new_local(interval);

    view! {
    <p class="control-context-note"
        data-idle-activity=move||current.with(|s|s.as_ref().and_then(|s|s.action).map(|a|
            match a{
                IdleAction::Idle=>"idle",
                IdleAction::Sleep=>"sleep",
                IdleAction::Sit=>"sit",
                IdleAction::Walk=>"walk"
            }
        ).unwrap_or("caged"))
        role="status">
        {move||
            if error.get(){
                "自然活動讀取失敗，稍後重試".to_owned()
            }else{
                current.with(|s|match s.as_ref().and_then(|s|s.action){
                    Some(IdleAction::Idle)=>"目前自然活動：發呆".into(),
                    Some(IdleAction::Sleep)=>"目前自然活動：睡覺".into(),
                    Some(IdleAction::Sit)=>"目前自然活動：坐下".into(),
                    Some(IdleAction::Walk)=>"目前自然活動：散步".into(),
                    None=>"尚未出籠".into()
                })
            }
        }
    </p>}
}

#[component]
pub(crate) fn IdleChoices(
    status: ArcRwSignal<Option<crate::bridge::ActivityStatus>>,
    busy: ReadSignal<bool>,
    action: Callback<pet_application::activity_control::ActivityAction>,
) -> impl IntoView {
    use pet_application::activity_control::ActivityAction::*;
    let ready = {
        let status = status.clone();
        move || busy.get() || status.with(Option::is_none)
    };
    let active_walk = {
        let status = status.clone();
        move || status.with(|s| s.as_ref().is_some_and(|s| s.roaming))
    };
    let active_sit = {
        let status = status.clone();
        move || status.with(|s| s.as_ref().is_some_and(|s| s.sitting))
    };
    let walk_label = {
        let status = status.clone();
        move || {
            status
                .with(|s| s.as_ref().and_then(|s| s.walk_label.clone()))
                .unwrap_or_else(|| "散步".into())
        }
    };
    let sit_label = {
        let status = status.clone();
        move || {
            status
                .with(|s| s.as_ref().and_then(|s| s.sit_label.clone()))
                .unwrap_or_else(|| "坐下".into())
        }
    };
    view! {
        <section class="control-activity" aria-label="所選寵物活動">
            <h3>"目前活動"</h3>
            <p>"只設定閒置亂數候選，不立即觸發活動。首次五分鐘無回應，之後每隔亂數5–15分鐘重選。"</p>
            <div class="control-activity-actions"
                role="group"
                aria-label="活動模式">
                <label>
                    <input type="radio"
                        name="idle-choice"
                        data-idle-choice="none"
                        disabled=ready.clone()
                        prop:checked={let state=status.clone();move||state.with(|s|s.as_ref().is_some_and(|s|!s.roaming&&!s.sitting))}
                        on:change=move |_|action.run(Clear)/>"都關閉"
                </label>
                <label>
                    <input type="radio"
                        name="idle-choice"
                        data-idle-choice="sit"
                        disabled=ready.clone()
                        prop:checked=active_sit
                        on:change=move |_|action.run(Sit)/>{sit_label}
                </label>
                <label>
                    <input type="radio"
                        name="idle-choice"
                        data-idle-choice="walk"
                        disabled=ready.clone()
                        prop:checked=active_walk
                        on:change=move |_|action.run(Walk)/>{walk_label}
                </label>
            </div>
        </section>
    }
}
