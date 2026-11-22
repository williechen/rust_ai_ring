use leptos::prelude::*;

pub(crate) fn cancel_on_escape(event: leptos::ev::KeyboardEvent, busy: bool, cancel: Callback<()>) {
    if event.key() == "Escape" && !event.is_composing() && !busy {
        event.prevent_default();
        cancel.run(());
    }
}
