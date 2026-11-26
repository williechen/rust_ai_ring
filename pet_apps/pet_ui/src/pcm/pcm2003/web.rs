use crate::bridge;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[component]
pub fn ChatBubble(
    selected: ReadSignal<Option<String>>,
    chat_open: ReadSignal<bool>,
    set_chat_open: WriteSignal<bool>,
    chat_input: ReadSignal<String>,
    set_chat_input: WriteSignal<String>,
    chat_loading: ReadSignal<bool>,
    set_chat_loading: WriteSignal<bool>,
    chat_response: ReadSignal<Option<String>>,
    set_chat_response: WriteSignal<Option<String>>,
    chat_error: ReadSignal<Option<String>>,
    set_chat_error: WriteSignal<Option<String>>,
) -> impl IntoView {
    let context = RwSignal::new(super::web_service::ChatContext::default());
    Effect::new(move |_| {
        let _ = selected.get();
        context.update(|c| c.select());
        set_chat_loading.set(false);
        set_chat_input.set(String::new());
        set_chat_response.set(None);
        set_chat_error.set(None);
    });
    let submit = move || {
        let Some(pet_id) = selected.get_untracked() else {
            return;
        };
        if chat_loading.get_untracked() {
            return;
        }

        let message = chat_input.get_untracked().trim().to_owned();

        if message.is_empty() {
            return;
        }

        let ticket = context.with_untracked(|c| c.ticket());
        set_chat_loading.set(true);
        set_chat_error.set(None);

        spawn_local(async move {
            let result = bridge::chat_with_pet(&pet_id, message).await;
            if selected.get_untracked().as_deref() != Some(pet_id.as_str())
                || !context.with_untracked(|c| c.accepts(ticket))
            {
                return;
            }
            match result {
                Ok(response) => {
                    set_chat_response.set(Some(response));
                    if context.with_untracked(|c| c.clears_draft(ticket)) {
                        set_chat_input.set(String::new());
                    }
                }
                Err(message) => set_chat_error.set(Some(message)),
            }
            set_chat_loading.set(false);
        });
    };

    view! {
        <Show
            when=move || chat_open.get()
            fallback=|| ()
        >
            <section class="chat-bubble">
                <div class="chat-bubble__header">
                    <strong>"跟小寵物說話"</strong>

                    <button
                        type="button"
                        class="chat-bubble__close"
                        aria-label="關閉對話"
                        on:click=move |_| {
                            set_chat_open.set(false);
                        }
                    >
                        "×"
                    </button>
                </div>

                <input
                    class="chat-bubble__input"
                    type="text"
                    placeholder="輸入一句話…"

                    prop:value=move || {
                        chat_input.get()
                    }

                    on:input=move |event| {
                        context.update(|c| c.edit());
                        set_chat_input.set(
                            event_target_value(&event)
                        );
                    }
                />

                <button
                    class="chat-bubble__send"
                    type="button"
                    prop:disabled=move || {chat_loading.get() || selected.get().is_none() || chat_input.get().trim().is_empty() }
                    on:click=move |_| submit()
                >
                    {move || {
                        if chat_loading.get() {
                            "送出中…"
                        } else {
                            "送出"
                        }
                    }}
                </button>

                <Show
                    when=move || chat_response.get().is_some()
                    fallback=|| ()
                >
                    <p class="chat-bubble__response">
                        {move || chat_response.get().unwrap_or_default()}
                    </p>
                </Show>

                <Show
                    when=move || chat_error.get().is_some()
                    fallback=|| ()
                >
                    <p class="chat-bubble__error">
                        {move || chat_error.get().unwrap_or_default()}
                    </p>
                </Show>

            </section>
        </Show>
    }
}
