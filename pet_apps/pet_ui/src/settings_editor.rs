use crate::bridge;
use leptos::prelude::*;
use leptos::task::spawn_local;
use pet_domain::profile::PetProfile;

#[derive(Clone, Default)]
struct Draft {
    name: String,
    species: String,
    likes: String,
    personality: String,
    interests: String,
}
impl From<PetProfile> for Draft {
    fn from(p: PetProfile) -> Self {
        Self {
            name: p.name,
            species: p.species,
            likes: p.likes.join("\n"),
            personality: p.personality.join("\n"),
            interests: p.interests.join("\n"),
        }
    }
}

fn tags(s: &str) -> Vec<String> {
    s.split([',', '，', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

impl Draft {
    fn profile(self) -> PetProfile {
        PetProfile {
            name: self.name,
            species: self.species,
            likes: tags(&self.likes),
            personality: tags(&self.personality),
            interests: tags(&self.interests),
            appearance: None,
        }
    }
}

fn current(seq: u64, captured: u64, selected: &Option<String>, id: &str) -> bool {
    seq == captured && selected.as_deref() == Some(id)
}

#[component]
fn SettingsFields(draft: RwSignal<Draft>) -> impl IntoView {
    view! {
     <label>"名字"<input name="settings-name" maxlength="32" required prop:value=move||draft.get().name on:input=move|e|draft.update(|d|d.name=event_target_value(&e))/></label>
     <label>"種類"<input name="settings-species" maxlength="32" required prop:value=move||draft.get().species on:input=move|e|draft.update(|d|d.species=event_target_value(&e))/></label>
     <label>"喜好"<textarea name="settings-likes" maxlength="8192" prop:value=move||draft.get().likes on:input=move|e|draft.update(|d|d.likes=event_target_value(&e))></textarea></label>
     <label>"個性"<textarea name="settings-personality" maxlength="8192" prop:value=move||draft.get().personality on:input=move|e|draft.update(|d|d.personality=event_target_value(&e))></textarea></label>
     <label>"興趣"<textarea name="settings-interests" maxlength="8192" prop:value=move||draft.get().interests on:input=move|e|draft.update(|d|d.interests=event_target_value(&e))></textarea></label>
    }
}
#[component]
pub fn PetSettingsEditor(
    selected_id: ReadSignal<Option<String>>,
    on_saved: Callback<(String, PetProfile)>,
) -> impl IntoView {
    let draft = RwSignal::new(Draft::default());
    let loaded = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let status = RwSignal::new("請先在管理區選擇寵物".to_string());
    let seq = RwSignal::new(0u64);
    let reload = RwSignal::new(0u64);
    Effect::new(move |_| {
        let id = selected_id.get();
        let _ = reload.get();
        seq.update(|n| *n += 1);
        let version = seq.get_untracked();
        loaded.set(None);
        draft.set(Draft::default());
        if let Some(id) = id {
            busy.set(true);
            status.set("正在讀取設定…".into());
            spawn_local(async move {
                let result = bridge::load_pet_settings(&id).await;
                if !current(
                    seq.get_untracked(),
                    version,
                    &selected_id.get_untracked(),
                    &id,
                ) {
                    return;
                }
                match result {
                    Ok(p) => {
                        draft.set(p.into());
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
    view! {
     <style>".pet-settings-form label{display:flex;flex-direction:column;gap:4px;margin:10px 0}.pet-settings-form textarea{min-height:72px;resize:vertical}.pet-settings-form fieldset{min-width:0}.pet-settings-form input,.pet-settings-form textarea{max-width:100%;box-sizing:border-box}"</style>
     <p>"目前支援種類：tortoise（烏龜）。喜好、個性、興趣以換行或逗號分隔；外觀覆寫尚未接通。"</p>
     <p role="status">{move||status.get()}</p>
     <button type="button" disabled=move||busy.get()||selected_id.get().is_none() on:click=move|_|reload.update(|n|*n+=1)>"重新讀取設定"</button>
     <form class="pet-settings-form" on:submit=move|e:leptos::ev::SubmitEvent|{
      e.prevent_default();if busy.get_untracked(){return;}
      let Some(id)=loaded.get_untracked().filter(|id|selected_id.get_untracked().as_ref()==Some(id)) else{return;};
      let version=seq.get_untracked();let p=draft.get_untracked().profile();busy.set(true);status.set("正在儲存設定…".into());
      spawn_local(async move{
       let result=bridge::save_pet_settings(&id,&p).await;
       if !current(seq.get_untracked(),version,&selected_id.get_untracked(),&id){return;}
       match result {Ok(p)=>{draft.set(p.clone().into());on_saved.run((id,p));status.set("已儲存設定".into());},Err(e)=>status.set(format!("儲存失敗：{e}"))};busy.set(false);
      });
     }>
      <fieldset disabled=move||busy.get()||loaded.get().is_none()><SettingsFields draft/><button type="submit">"儲存五欄設定"</button></fieldset>
     </form>
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
