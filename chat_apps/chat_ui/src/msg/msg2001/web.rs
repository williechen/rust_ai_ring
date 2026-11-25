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
            <button type="button"
                class="history-reload"
                disabled=move || loading.get()
                on:click=move |_| reload.run(())>
                "重試歷史訊息"
            </button>
        </Show>
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn restore_composer(input: &web_sys::HtmlTextAreaElement) {
    let _ = web_sys::HtmlElement::style(input).set_property("height", "44px");
    let _ = input.focus();
}

#[component]
pub(crate) fn ReconnectControl(
    failed: RwSignal<bool>,
    busy: RwSignal<bool>,
    reconnect: Callback<()>,
) -> impl IntoView {
    view! {
        <Show when=move||failed.get()>
            <button type="button"
                class="history-reload"
                data-action="reconnect"
                disabled=move||busy.get()
                on:click=move |_|reconnect.run(())>
                {move||if busy.get(){"連線中…"}else{"重新連線"}}
            </button>
        </Show>
    }
}
