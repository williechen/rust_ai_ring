use leptos::prelude::*;

pub(crate) fn cancel_on_escape(event: leptos::ev::KeyboardEvent, busy: bool, cancel: Callback<()>) {
    if event.key() == "Escape" && !event.is_composing() && !busy {
        event.prevent_default();
        cancel.run(());
    }
}

pub(crate) fn clear_search(
    query: RwSignal<String>,
    page: RwSignal<usize>,
    input: NodeRef<leptos::html::Input>,
) {
    query.set(String::new());
    page.set(1);
    if let Some(input) = input.get_untracked() {
        let _ = input.focus();
    }
}
