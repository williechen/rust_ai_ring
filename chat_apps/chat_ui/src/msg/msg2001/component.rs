use leptos::prelude::*;

#[component]
pub(crate) fn HistoryFeedback(
    status: ReadSignal<String>,
    failed: RwSignal<bool>,
    loading: RwSignal<bool>,
    reload: Callback<()>,
) -> impl IntoView {
    view! {
        <p class="history-disclosure" role="status">{move || status.get()}</p>
        <Show when=move || failed.get()>
            <button type="button" class="history-reload" disabled=move || loading.get()
                on:click=move |_| reload.run(())>"重試歷史訊息"</button>
        </Show>
    }
}
