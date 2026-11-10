use crate::bridge;
use leptos::prelude::*;
use pet_domain::{PetCommand, PetSnapshot};
use wasm_bindgen_futures::spawn_local;

use crate::component::apply_snapshot;
use crate::saved_pet::SavedPet;

fn dispatch_command(
    command: PetCommand,
    active_pet: ReadSignal<Option<SavedPet>>,
    profile_events: RwSignal<u64>,
    snapshot: ReadSignal<Option<PetSnapshot>>,
    set_snapshot: WriteSignal<Option<PetSnapshot>>,
    set_error: WriteSignal<Option<String>>,
) {
    let Some(id) = active_pet.with_untracked(|pet| pet.as_ref().map(|pet| pet.id.clone())) else {
        return;
    };
    if snapshot.with_untracked(Option::is_none) {
        return;
    }
    let generation = profile_events.get_untracked();
    set_error.set(None);

    spawn_local(async move {
        let result = bridge::send_pet_command_for(command, Some(id.clone())).await;
        if profile_events.get_untracked() != generation
            || !active_pet.with_untracked(|pet| pet.as_ref().is_some_and(|pet| pet.id == id))
        {
            return;
        }
        match result {
            Ok(incoming) => {
                apply_snapshot(incoming, snapshot, set_snapshot);
            }

            Err(message) => {
                set_error.set(Some(message));
            }
        }
    });
}

#[component]
pub fn PetControls(
    active_pet: ReadSignal<Option<SavedPet>>,
    profile_events: RwSignal<u64>,
    snapshot: ReadSignal<Option<PetSnapshot>>,
    set_snapshot: WriteSignal<Option<PetSnapshot>>,
    set_error: WriteSignal<Option<String>>,
) -> impl IntoView {
    let waiting = move || active_pet.with(Option::is_none) || snapshot.with(Option::is_none);
    let run = move |command| {
        dispatch_command(
            command,
            active_pet,
            profile_events,
            snapshot,
            set_snapshot,
            set_error,
        );
    };
    view! {
        <Show when=waiting>
            <p class="control-empty" role="status">"尚未取得生活窗狀態，請確認寵物已出籠。"</p>
        </Show>
        <section class="pet-controls">
            <button type="button" disabled=waiting on:click=move |_| run(PetCommand::Interact)>
                "互動"
            </button>

            <button type="button" disabled=waiting on:click=move |_| run(PetCommand::FinishInteraction)>
                "結束"
            </button>

            <button type="button" disabled=waiting on:click=move |_| run(PetCommand::Sleep)>
                "睡覺"
            </button>

             <button type="button" disabled=waiting on:click=move |_| run(PetCommand::Wake)>
                "醒來"
            </button>
        </section>
    }
}
