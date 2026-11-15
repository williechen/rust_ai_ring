use crate::bridge;
use leptos::prelude::*;
use leptos::task::spawn_local;
use pet_application::settings_draft::SettingsDraft as Draft;
use pet_domain::profile::PetProfile;

#[cfg(test)]
fn current(seq: u64, captured: u64, selected: &Option<String>, id: &str) -> bool {
    let mut request = pet_application::settings_request::SettingsRequest::default();
    for _ in 0..seq {
        request.begin(selected.clone());
    }
    request.accepts(captured, selected, id)
}

#[component]
fn SettingsFields(draft: RwSignal<Draft>) -> impl IntoView {
    view! {
    <label>"名字"
        <input
            name="settings-name"
            maxlength="32"
            required
            prop:value=move||draft.get().name
            on:input=move|e|draft.update(|d|d.name=event_target_value(&e))/>
    </label>
    <label>"種類"
        <input
            name="settings-species"
            maxlength="32"
            required
            prop:value=move||draft.get().species
            on:input=move|e|draft.update(|d|d.species=event_target_value(&e))/>
    </label>
    <label>"喜好"
        <textarea
            name="settings-likes"
            maxlength="8192"
            prop:value=move||draft.get().likes
            on:input=move|e|draft.update(|d|d.likes=event_target_value(&e))></textarea>
    </label>
    <label>"個性"
        <textarea
            name="settings-personality"
            maxlength="8192"
            prop:value=move||draft.get().personality
            on:input=move|e|draft.update(|d|d.personality=event_target_value(&e))></textarea>
    </label>
    <label>"興趣"
        <textarea
            name="settings-interests"
            maxlength="8192"
            prop:value=move||draft.get().interests
            on:input=move|e|draft.update(|d|d.interests=event_target_value(&e))></textarea>
    </label>
    }
}

#[component]
pub fn PetSettingsEditor(
    selected_id: ReadSignal<Option<String>>,
    on_saved: Callback<(String, PetProfile)>,
    dirty: RwSignal<bool>,
    busy: RwSignal<bool>,
    save_request: RwSignal<u64>,
    discard_request: RwSignal<u64>,
    navigation_pending: Signal<bool>,
) -> impl IntoView {
    let draft = RwSignal::new(Draft::default());
    let saved = RwSignal::new(Draft::default());
    let loaded = RwSignal::new(None::<String>);

    let status = RwSignal::new("請先在管理區選擇寵物".to_string());
    let request = RwSignal::new(pet_application::settings_request::SettingsRequest::default());
    let reload = RwSignal::new(0u64);
    let confirm_reload = RwSignal::new(false);
    Effect::new(move |_| {
        let id = selected_id.get();
        let _ = reload.get();
        let version = request.try_update(|r| r.begin(id.clone())).unwrap();
        loaded.set(None);
        confirm_reload.set(false);
        draft.set(Draft::default());
        if let Some(id) = id {
            busy.set(true);
            status.set("正在讀取設定…".into());
            spawn_local(async move {
                let result = bridge::load_pet_settings(&id).await;
                if !request
                    .with_untracked(|r| r.accepts(version, &selected_id.get_untracked(), &id))
                {
                    return;
                }
                match result {
                    Ok(p) => {
                        let incoming: Draft = p.into();
                        saved.set(incoming.clone());
                        draft.set(incoming);
                        loaded.set(Some(id));
                        status.set("已載入所選寵物設定".into());
                    }
                    Err(e) => status.set(format!("讀取失敗：{e}")),
                };
                busy.set(false);
            });
        } else {
            busy.set(false);
            status.set("請先在管理區選擇寵物".into());
        }
    });
    Effect::new(move |_| {
        dirty.set(loaded.get().is_some() && draft.get() != saved.get());
    });
    Effect::new(move |_| {
        let _ = discard_request.get();
        draft.set(saved.get_untracked());
        confirm_reload.set(false);
    });
    Effect::new(move |_| {
        if navigation_pending.get() {
            confirm_reload.set(false);
        }
    });
    let save = Callback::new(move |_: ()| {
        if busy.get_untracked() || confirm_reload.get_untracked() {
            return;
        }
        let Some(id) = loaded
            .get_untracked()
            .filter(|id| selected_id.get_untracked().as_ref() == Some(id))
        else {
            return;
        };
        let version = request.with_untracked(|r| r.generation());
        let p = draft.get_untracked().profile();
        busy.set(true);
        status.set("正在儲存設定…".into());
        spawn_local(async move {
            let result = bridge::save_pet_settings(&id, &p).await;
            if !request.with_untracked(|r| r.accepts(version, &selected_id.get_untracked(), &id)) {
                return;
            }
            match result {
                Ok(p) => {
                    let committed: Draft = p.clone().into();
                    saved.set(committed.clone());
                    draft.set(committed);
                    on_saved.run((id, p));
                    status.set("已儲存設定".into());
                }
                Err(e) => status.set(format!("儲存失敗：{e}")),
            };
            busy.set(false);
        });
    });
    Effect::new(move |_| {
        if save_request.get() > 0 {
            save.run(());
        }
    });
    view! {
       <style>".pet-settings-form label{display:flex;flex-direction:column;gap:4px;margin:10px 0}.pet-settings-form textarea{min-height:72px;resize:vertical}.pet-settings-form fieldset{min-width:0}.pet-settings-form input,.pet-settings-form textarea{max-width:100%;box-sizing:border-box}"</style>
       <p class="control-context-note">"喜好、個性和興趣可用換行或逗號分隔。"</p>
       <p role="status">{move||status.get()}</p>
       <button type="button" disabled=move||busy.get()||navigation_pending.get()||selected_id.get().is_none() on:click=move|_|{
           if draft.get_untracked() != saved.get_untracked() && loaded.get_untracked().is_some() {
               confirm_reload.set(true);
           } else { reload.update(|n|*n+=1); }
       }>"重新讀取設定"</button>
       <Show when=move||confirm_reload.get()>
         <ReloadConfirmation
            on_discard=Callback::new(move|_|{confirm_reload.set(false);reload.update(|n|*n+=1);})
            on_keep=Callback::new(move|_|confirm_reload.set(false)) />
       </Show>
       <form class="pet-settings-form" on:submit=move|e:leptos::ev::SubmitEvent|{ e.prevent_default(); if !navigation_pending.get_untracked(){save.run(());} }>
       <fieldset disabled=move||busy.get()||confirm_reload.get()||navigation_pending.get()||loaded.get().is_none()>
      <SettingsFields draft />
      <details><summary>"外觀覆寫（選用）"</summary><p>"目前尚未接通外觀儲存。"</p></details>
      <div class="settings-actions">
          <button type="button" on:click=move |_| {
              draft.set(saved.get_untracked());
              status.set("已取消未儲存變更".into());
          }>"取消變更"</button>
          <button class="control-primary" type="submit">"儲存設定"</button>
      </div>
    </fieldset>
       </form>
      }
}

#[component]
fn ReloadConfirmation(on_discard: Callback<()>, on_keep: Callback<()>) -> impl IntoView {
    view! {
        <section role="alertdialog" aria-modal="false" aria-labelledby="settings-reload-title">
            <h3 id="settings-reload-title">"有未儲存的設定"</h3>
            <p>"重新讀取會捨棄目前草稿，是否繼續？"</p>
            <button type="button" on:click=move|_|on_keep.run(())>"繼續編輯"</button>
            <button type="button" on:click=move|_|on_discard.run(())>"捨棄並重新讀取"</button>
        </section>
    }
}

#[cfg(test)]
mod day62_reload_tests {
    use super::*;
    #[test]
    fn reload_confirmation_has_explicit_keep_and_discard_actions() {
        Owner::new().with(|| {
            let html=view! { <ReloadConfirmation on_discard=Callback::new(|_|{}) on_keep=Callback::new(|_|{}) /> }.to_html();
            assert!(html.contains("role=\"alertdialog\""));
            assert!(html.contains("繼續編輯"));
            assert!(html.contains("捨棄並重新讀取"));
            assert!(!html.contains("type=\"submit\""));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_response_cannot_replace_new_target_or_new_request() {
        assert!(current(2, 2, &Some("b".into()), "b"));
        assert!(!current(2, 1, &Some("b".into()), "b"));
        assert!(!current(2, 2, &Some("b".into()), "a"));
        assert!(!current(2, 2, &None, "b"));
    }
    #[test]
    fn actual_fields_have_five_accessible_controls() {
        Owner::new().with(|| {
            let draft = RwSignal::new(Draft::default());
            let html = view! {<SettingsFields draft/>}.to_html();
            for name in ["name", "species", "likes", "personality", "interests"] {
                assert!(html.contains(&format!("name=\"settings-{name}\"")));
            }
            assert_eq!(html.matches("<label").count(), 5);
        });
    }
    #[test]
    fn text_input_builds_five_field_payload() {
        let p = Draft {
            name: "小龜".into(),
            species: "tortoise".into(),
            likes: "太陽，草\n水".into(),
            ..Default::default()
        }
        .profile();
        assert_eq!(p.likes, vec!["太陽", "草", "水"]);
        assert!(p.appearance.is_none());
    }
}
