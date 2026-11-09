mod activity_target;
mod bridge;
mod cage_status;
mod component;
mod control_activity;
mod control_navigation;
#[cfg(test)]
mod duplicate_preview;
mod file_browser;
mod file_comparison;
#[cfg(test)]
mod file_scan_view;
pub mod management_selection;
mod saved_pet;
mod scan_generation;
mod settings_editor;

pub use component::*;

use leptos::mount::mount_to_body;
use leptos::{prelude::*, task::spawn_local};
use pet_domain::PetSnapshot;
use std::collections::HashMap;

use crate::cage_status::record_cage_status;
use crate::saved_pet::SavedPet;
use control_navigation::{ControlNavigation, ControlPage};
use pet_application::lifecycle::StartupRestoreFailure;

fn main() {
    mount_to_body(|| {
        view! {
            <App />
        }
    });
}

#[component]
fn App() -> impl IntoView {
    let (snapshot, set_snapshot) = signal::<Option<PetSnapshot>>(None);

    let (error, set_error) = signal::<Option<String>>(None);

    let (blinking, set_blinking) = signal(false);

    let (animation_phase, set_animation_phase) = signal(false);

    let (chat_open, set_chat_open) = signal(false);
    let (chat_input, set_chat_input) = signal(String::new());

    let (chat_loading, set_chat_loading) = signal(false);
    let (chat_response, set_chat_response) = signal::<Option<String>>(None);
    let (chat_error, set_chat_error) = signal::<Option<String>>(None);

    let (file_directory, set_file_directory) = signal(None::<Vec<String>>);

    let (new_pet_name, set_new_pet_name) = signal(String::new());
    let (new_pet_feedback, set_new_pet_feedback) = signal(String::new());

    let (saved_pets, set_saved_pets) = signal(Vec::<SavedPet>::new());
    let (pets_busy, set_pets_busy) = signal(false);
    let cage_status = RwSignal::new(HashMap::<String, String>::new());
    let startup_failures = RwSignal::new(HashMap::<String, StartupRestoreFailure>::new());

    let (management_pet_id, set_management_pet_id) = signal(None::<String>);

    let (active_pet, set_active_pet) = signal::<Option<SavedPet>>(None);
    let profile_events = RwSignal::new(0_u64);
    spawn_local(async move {
        if let Err(message) = bridge::listen_pet_profile(move |pet| {
            profile_events.update(|n| *n += 1);
            set_active_pet.set(Some(pet));
            set_snapshot.set(None);
            set_error.set(None);
            spawn_local(async move {
                if let Ok(incoming) = bridge::get_pet_state().await {
                    set_snapshot.set(Some(incoming));
                }
            });
        })
        .await
        {
            set_error.set(Some(message));
        }
        let generation = profile_events.get_untracked();
        match bridge::active_pet().await {
            Ok(pet) if profile_events.get_untracked() == generation => set_active_pet.set(pet),
            Ok(_) => (), // 載入期間已收到較新的選取事件，不覆寫。
            Err(message) => set_error.set(Some(message)),
        }
    });

    if !bridge::is_pet_window() {
        // 先訂閱再讀取快照，以免事件早於管理頁初始化而遺失。
        spawn_local(async move {
            if let Err(message) = bridge::listen_startup_restore_failures(move |failure| {
                startup_failures.update(|rows| {
                    rows.insert(failure.id.clone(), failure);
                });
            })
            .await
            {
                set_new_pet_feedback.set(format!("無法監聽啟動恢復失敗：{message}"));
            }
            match bridge::startup_restore_failures().await {
                Ok(rows) => startup_failures.update(|current| {
                    for failure in rows {
                        current.entry(failure.id.clone()).or_insert(failure);
                    }
                }),
                Err(message) => {
                    set_new_pet_feedback.set(format!("無法讀取啟動恢復失敗：{message}"))
                }
            }
        });
        set_pets_busy.set(true);
        spawn_local(async move {
            match bridge::list_pets().await {
                Ok(rows) => {
                    set_saved_pets.set(rows);
                    cage_status.set(HashMap::new());
                    set_new_pet_feedback.set("已載入儲存清單".into());
                }
                Err(error) => set_new_pet_feedback.set(error),
            }
            set_pets_busy.set(false);
        });
    }

    {
        let snapshot_reader = snapshot;
        let snapshot_writer = set_snapshot;
        let error_writer = set_error;

        spawn_local(async move {
            let result = bridge::listen_pet_state(move |incoming| {
                apply_snapshot(incoming, snapshot_reader, snapshot_writer);
            })
            .await;

            if let Err(message) = result {
                error_writer.set(Some(message));
            }
        });

        spawn_local(async move {
            match bridge::get_pet_state().await {
                Ok(incoming) => {
                    apply_snapshot(incoming, snapshot_reader, snapshot_writer);
                }

                Err(message) => {
                    error_writer.set(Some(message));
                }
            }
        });
    }

    if bridge::is_pet_window() {
        return view! {
            <main class="pet-life">
                <DragHandle />
                <BlinkController
                    snapshot
                    set_blinking
                />
                <AnimationClock set_animation_phase />

                <p title=move || active_pet.get().map(|p|p.id).unwrap_or_default()>
                    {move || active_pet.get().map(|p|p.name).unwrap_or_else(|| "請在控制台選擇生活窗寵物".into())}
                </p>
                <Show when=move || active_pet.get().is_some() fallback=|| ()>
                    <PetStatus snapshot blinking animation_phase />
                </Show>
                <button type="button" on:click=move |_| spawn_local(async move {
                    if let Err(e)=bridge::cage_saved_pet().await {set_error.set(Some(e));}
               })>"回籠子"</button>
                <p role="alert">{move || error.get().unwrap_or_default()}</p>
                <button
                    type="button"
                    on:click=move |_| spawn_local(async move{
                        if bridge::open_controls().await.is_err(){
                            set_error.set(Some("無法開啟控制台".into()));
                        }
                    })
                >"控制台"</button>
            </main>
        }
        .into_any();
    }

    let page = RwSignal::new(ControlPage::Interaction);
    let (startup_view, set_startup_view) = signal(false);
    view! {
        <main class="control-shell">
            <style>{include_str!("control.css")}</style>
           <header class="control-heading">
                <span class="control-mark" aria-hidden="true">"P"</span>
                <h1>"Pet｜桌面陪伴"</h1>
            </header>
            <ControlNavigation page />
            <div class="control-context" hidden=move || !matches!(page.get(), ControlPage::Interaction | ControlPage::Settings | ControlPage::Memory)>
                <label class="control-pet-picker">
                    <span>"目前寵物"</span>
                    <select disabled=move || pets_busy.get() || saved_pets.with(Vec::is_empty)
                        prop:value=move || management_pet_id.get().unwrap_or_default()
                        on:change=move |event| {
                            let id = event_target_value(&event);
                            if id.is_empty() {
                                set_management_pet_id.set(None);
                            } else {
                                let _ = management_selection::choose_management_pet(
                                    &id, &saved_pets.get_untracked(), set_management_pet_id,
                                );
                            }
                        }>
                        <option value="">"選擇寵物"</option>
                        <For each=move || saved_pets.get() key=|pet| (pet.id.clone(), pet.name.clone())
                            children=move |pet| {
                                let id = pet.id.clone();
                                view! {
                                    <option value=pet.id prop:selected=move || management_pet_id.with(|selected| selected.as_ref() == Some(&id))>{pet.name}</option>
                                }
                            } />
                    </select>
                </label>
                <p class="control-context-note">"選擇對象只會切換管理畫面。"</p>
            </div>

            <section id="pet-interaction" role="tabpanel" aria-labelledby="pet-interaction-tab" tabindex="0"
                hidden=move || page.get() != ControlPage::Interaction>
            <div class="control-section-heading">
                <div>
                    <h2 id="pet-interaction-title">"陪伴與互動"</h2>
                    <p>"切換活動只會影響所選寵物。"</p>
                </div>
            </div>

            <For each=move || {
                activity_target::activity_target(&saved_pets.get(), management_pet_id.get().as_deref())
                    .into_iter().collect::<Vec<_>>()
            } key=String::clone
                children=move |id| view! {
                    {
                        let display_id = id.clone();
                        let pet = Signal::derive(move || saved_pets.with(|pets| pets.iter().find(|row| row.id == display_id).cloned()));
                        view! { <control_activity::ActivityWorkspace id pet busy=pets_busy set_busy=set_pets_busy page /> }
                    }
                } />
            <Show when=move || management_pet_id.with(Option::is_none)>
                <p class="control-empty">
                    {move || if saved_pets.with(Vec::is_empty) {
                        "請先從管理區新增寵物。"
                    } else {
                        "請先選擇一隻寵物，查看生活窗預覽和目前活動。"
                    }}
                </p>
            </Show>

            <details class="control-live-actions">
                <summary>"生活窗互動與聊天"</summary>
                <ErrorMessage error />
                <p>{move || active_pet.get().map(|pet| format!("作用對象：{}", pet.name)).unwrap_or_else(|| "尚無作用中的生活窗。".into())}</p>
                <PetControls snapshot set_snapshot set_error />
            <button
                class="chat-trigger"
                type="button"
                on:click=move |_| {set_chat_open.set(true);}
            >"聊天"</button>

            <ChatBubble
                 chat_open
                 set_chat_open
                 chat_input
                 set_chat_input
                 chat_loading
                 set_chat_loading
                 chat_response
                 set_chat_response
                 chat_error
                 set_chat_error
            />
            </details>

            </section>
            <section id="pet-settings" role="tabpanel" aria-labelledby="pet-settings-tab" tabindex="0"
                hidden=move || page.get() != ControlPage::Settings>
                <h2 id="pet-settings-title">{move || saved_pets.with(|rows| {
                    let id = management_pet_id.get();
                    rows.iter().find(|pet| Some(&pet.id) == id.as_ref())
                        .map(|pet| format!("{}的設定", pet.name)).unwrap_or_else(|| "寵物設定".into())
                })}</h2>
                <p class="control-context-note">"編輯這隻寵物的資料，儲存前不會寫入。"</p>
                <div class="settings-jump" role="group" aria-label="設定分類">
                    <button type="button" aria-pressed=move || (!startup_view.get()).to_string()
                        aria-controls="pet-settings-profile" on:click=move |_| set_startup_view.set(false)>"資料與外觀"</button>
                    <button type="button" aria-pressed=move || startup_view.get().to_string()
                        aria-controls="pet-startup" on:click=move |_| set_startup_view.set(true)>"啟動預設"</button>
                </div>
                <div id="pet-settings-profile" hidden=move || startup_view.get()>
                 <settings_editor::PetSettingsEditor selected_id=management_pet_id on_saved=Callback::new(move |(id,p):(String,pet_domain::profile::PetProfile)|{
                    set_saved_pets.update(|rows|{if let Some(row)=rows.iter_mut().find(|r|r.id==id){row.name=p.name.clone();row.species=p.species.clone();}});
                    if active_pet.get_untracked().as_ref().is_some_and(|pet|pet.id==id){set_active_pet.set(Some(SavedPet{id,name:p.name,species:p.species}));}
                })/>
                </div>
                <section id="pet-startup" hidden=move || !startup_view.get()>
                    <h3>"啟動預設"</h3>
                    <p class="control-context-note">"啟動預設與目前出籠狀態分開。"</p>
                    <control_activity::StartupPreferences selected=management_pet_id />
                </section>
            </section>
            <section id="pet-memory" role="tabpanel" aria-labelledby="pet-memory-tab" tabindex="0"
                hidden=move || page.get() != ControlPage::Memory>
                <h2 id="pet-memory-title">"記憶"</h2>
                <p>"記憶查詢與管理尚未接通。"</p>
            </section>
            <section id="pet-files" role="tabpanel" aria-labelledby="pet-files-tab" tabindex="0"
                hidden=move || page.get() != ControlPage::Files>
            <h2 id="pet-files-title">"檔案管理"</h2>
            <p class="control-context-note">"瀏覽本機已授權的目錄。"</p>
            <file_browser::FileBrowser active=Signal::derive(move || page.get() == ControlPage::Files)
                on_directory=Callback::new(move |directory| set_file_directory.set(directory)) />
            <fieldset class="file-operation-layout" disabled aria-describedby="file-operations-status">
                <legend>"檔案操作"</legend>
                <p id="file-operations-status">"按鈕位置已就緒，待接線。"</p>
                <button type="button">"開啟"</button>
                <button type="button">"新增資料夾"</button>
                <button type="button">"重新命名"</button>
                <button type="button">"複製"</button>
                <button type="button">"移動"</button>
                <button type="button">"移至垃圾桶"</button>
            </fieldset>
            <file_comparison::FileComparison directory=file_directory />
            </section>
            <section id="pet-management" role="tabpanel" aria-labelledby="pet-management-tab" tabindex="0"
                hidden=move || page.get() != ControlPage::Management>
                <h2 id="pet-management-title">"管理"</h2>
                <Show when=move || !startup_failures.get().is_empty() fallback=|| ()>
                    <section role="alert" aria-label="啟動恢復失敗">
                        <h3>"部分寵物啟動時未能恢復"</h3>
                        <ul>
                            <For each=move || {
                                let mut rows = startup_failures.get().into_values().collect::<Vec<_>>();
                                rows.sort_by(|a, b| a.id.cmp(&b.id));
                                rows
                            } key=|failure| failure.id.clone()
                            children=move |failure| view! {
                                <li>{format!("{}（{}）：{}", failure.name, failure.id, failure.reason)}</li>
                            }/>
                        </ul>
                    </section>
                </Show>
                <h3>"新增寵物"</h3>
                <label>"寵物名稱"
                    <input
                        prop:value=move || new_pet_name.get()
                        on:input=move |ev| set_new_pet_name.set(event_target_value(&ev)) />
                </label>
                <button
                    type="button"
                    disabled=move || pets_busy.get()
                    on:click=move |_| {
                        if pets_busy.get_untracked() { return; }
                        set_pets_busy.set(true);
                        let proposed = new_pet_name.get_untracked();
                        // apps/desktop_pet/frontend/src/main.rs
                        // 僅替換「建立寵物」按鈕原有的 spawn_local(async move { ... }) 閉包。
                        spawn_local(async move {
                            match bridge::create_pet(proposed).await {
                                Ok(created) => {
                                    // 先更新清單，不因後續開窗失敗重送 INSERT。
                                    match bridge::list_pets().await {
                                        Ok(rows) => {
                                            management_selection::reconcile_management_pet(
                                                &rows, management_pet_id, set_management_pet_id
                                            );
                                            set_saved_pets.set(rows);
                                        },
                                        Err(_) => set_new_pet_feedback.set(
                                            "寵物已儲存，但清單讀回失敗；請重新載入，勿重送建立。".into()
                                        ),
                                    }
                                    // select_pet Tauri command 既有實作會先儲存
                                    // local_pet_selection，再呼叫 pet_windows::open。
                                    match bridge::select_pet(&created.id).await {
                                        Ok(selected) => {
                                            set_active_pet.set(Some(selected));
                                            set_new_pet_name.set(String::new());
                                            set_new_pet_feedback.set(
                                                format!("已建立並出籠：{}（{}）",created.name,created.id)
                                            );
                                        }
                                        Err(error) => set_new_pet_feedback.set(format!(
                                            "寵物已儲存：{}（{}）；選取／出籠未完成：{}。請從清單重試，不要再建立。",
                                            created.name, created.id, error
                                        )),
                                    }
                                }
                                Err(error) => set_new_pet_feedback.set(format!("建立寵物失敗：{error}")),
                            }
                            set_pets_busy.set(false);
                        });
                    }
                >"建立寵物"</button>
                <p role="status">{move || new_pet_feedback.get()}</p>
                 <h3>"已儲存寵物"</h3>
                <button type="button" disabled=move || pets_busy.get()
                    on:click=move |_| {
                        if pets_busy.get_untracked() { return; }
                        set_pets_busy.set(true);
                        spawn_local(async move {
                            match bridge::list_pets().await {
                                Ok(rows) => {
                                    // 只在已提交的新清單中確認管理對象是否仍存在。
                                    let removed = management_selection::reconcile_management_pet(
                                        &rows, management_pet_id, set_management_pet_id
                                    );
                                    set_saved_pets.set(rows);
                                    set_new_pet_feedback.set(if removed {
                                        "原管理對象已不在清單，請重新選擇".into()
                                    } else {
                                        "已重新載入清單".into()
                                    });
                                }
                                Err(error) => set_new_pet_feedback.set(format!("讀取清單失敗：{error}")),
                            }
                            set_pets_busy.set(false);
                        });
                    }>"重新載入清單"</button>
                <Show when=move || pets_busy.get() fallback=|| ()><p>"處理中…"</p></Show>
                <ul>
                    <For each=move || saved_pets.get() key=|pet| pet.id.clone()
                        children=move |pet| {
                            let id=pet.id.clone();
                            let open_id=id.clone();
                            let cage_id=id.clone();
                            let find_id=id.clone();
                            let status_id=id.clone();
                            let display_id=id.clone();
                            let selected_id=id.clone();
                            let rename_scope=id.clone();
                            let (rename_input, set_rename_input) = signal(pet.name.clone());
                            let (rename_id, set_rename_id) = signal(pet.id.clone());
                            view! { <li>{move || saved_pets.get().iter().find(|row| row.id == pet.id).map(|row| format!("{}／{}（{}）", row.name, row.species, row.id)).unwrap_or_default()}
                                <p role="status">{move || if management_pet_id.get().as_deref()==Some(selected_id.as_str()) { "目前管理對象" } else { "" }}</p>
                                <Show when=move || management_pet_id.get().as_deref()==Some(rename_scope.as_str()) fallback=|| ()>
                                <label>"修改名稱"
                                    <input aria-label="寵物新名稱" maxlength="64"
                                        prop:value=move || rename_input.get()
                                        on:input=move |event| set_rename_input.set(event_target_value(&event)) />
                                </label>
                                <button type="button" disabled=move || pets_busy.get()
                                    on:click=move |_| {
                                        if pets_busy.get_untracked() { return; }
                                        set_pets_busy.set(true);
                                        let id = rename_id.get_untracked();
                                        let proposed = rename_input.get_untracked();
                                        spawn_local(async move {
                                            match bridge::rename_pet(&id, &proposed).await {
                                                Ok(updated) => {
                                                    set_saved_pets.update(|rows| {
                                                        if let Some(row) = rows.iter_mut().find(|p| p.id == id) {
                                                            row.name = updated.name.clone();
                                                        }
                                                    });
                                                    if active_pet.get_untracked().as_ref().is_some_and(|p| p.id == id) {
                                                        set_active_pet.set(Some(updated));
                                                    }
                                                    set_new_pet_feedback.set("已更新寵物名稱".into());
                                                }
                                                Err(error) => set_new_pet_feedback.set(format!("更名失敗：{error}")),
                                            }
                                            set_pets_busy.set(false);
                                        });
                                    }>"儲存名稱"</button>
                                </Show>
                                <button type="button" disabled=move || pets_busy.get()
                                    on:click=move |_| {
                                        match management_selection::choose_management_pet(&id,&saved_pets.get_untracked(),set_management_pet_id) {
                                            Ok(()) => set_new_pet_feedback.set("已選為管理對象；出入籠狀態保持不變".into()),
                                            Err(message) => set_new_pet_feedback.set(message),
                                        }
                                    }>"選為管理對象"</button>
                                <button type="button" disabled=move || pets_busy.get()
                                    on:click=move |_| {
                                        if pets_busy.get_untracked() {return;}
                                        set_pets_busy.set(true);let id=open_id.clone();
                                        spawn_local(async move {
                                            match bridge::open_saved_pet(&id).await {
                                                Ok(())=>{
                                                    cage_status.update(|statuses| {
                                                        statuses.remove(&id);
                                                    });
                                                    set_new_pet_feedback.set("已出籠；重複操作只聚焦既有生活窗".into());
                                                }
                                                ,Err(e)=>set_new_pet_feedback.set(e)
                                            }
                                            set_pets_busy.set(false);
                                        });
                                    }>"出籠"</button>
                                <button type="button" disabled=move || pets_busy.get()
                                    on:click=move |_| {
                                        if pets_busy.get_untracked() { return; }
                                        set_pets_busy.set(true);
                                        let id = status_id.clone();
                                        spawn_local(async move {
                                            let result = bridge::pet_cage_status_by_id(&id).await;
                                            cage_status.update(|statuses| record_cage_status(statuses, &id, result));
                                            set_pets_busy.set(false);
                                        });
                                    }>"查詢回籠狀態"</button>
                                <p role="status" aria-live="polite">{move || cage_status.get().get(&display_id).cloned().unwrap_or_default()}</p>
                                <button type="button" disabled=move || pets_busy.get()
                                    on:click=move |_| {
                                        if pets_busy.get_untracked() { return; }
                                        set_pets_busy.set(true);
                                        let id = cage_id.clone();
                                        spawn_local(async move {
                                            match bridge::cage_saved_pet_by_id(&id).await {
                                                Ok(()) => {
                                                    cage_status.update(|statuses| { statuses.remove(&id); });
                                                    if active_pet.get_untracked().as_ref()
                                                        .is_some_and(|pet| pet.id == id) {
                                                        set_active_pet.set(None);
                                                        set_snapshot.set(None);
                                                    }
                                                    set_new_pet_feedback.set("該隻已回籠；其他寵物保持原狀".into());
                                                }
                                                Err(error) => set_new_pet_feedback.set(format!("回籠失敗：{error}")),
                                            }
                                            set_pets_busy.set(false);
                                        });
                                    }>"回籠這隻"</button>
                                    <button type="button" disabled=move || pets_busy.get()
                                        on:click=move |_| {
                                            if pets_busy.get_untracked() { return; }
                                            set_pets_busy.set(true);
                                            let id = find_id.clone();
                                            spawn_local(async move {
                                                match bridge::find_saved_pet(&id).await {
                                                    Ok(()) => set_new_pet_feedback.set("已找回該隻生活窗".into()),
                                                    Err(e) => set_new_pet_feedback.set(e),
                                                }
                                                set_pets_busy.set(false);
                                            });
                                        }>"找回寵物"</button>
                            </li> }
                        } />
                </ul>
            </section>

<section id="pet-updates" role="tabpanel" aria-labelledby="pet-updates-tab" tabindex="0"
                hidden=move || page.get() != ControlPage::Updates>
                <h2 id="pet-updates-title">"更新"</h2>
                <p>"版本檢查與簽章更新尚未接通。"</p>
            </section>

            <footer class="control-footer">
                <span>"關閉控制台，陪伴繼續"</span>
                <button type="button" on:click=move |_| spawn_local(async move {
                    let _ = bridge::quit_pet().await;
                })>"結束寵物程式"</button>
            </footer>
        </main>
    }.into_any()
}

#[component]
fn ErrorMessage(error: ReadSignal<Option<String>>) -> impl IntoView {
    view! {
        <Show
            when=move || error.get().is_some()
            fallback=|| ()
        >
            <p class="error">
                {move || {
                    error
                        .get()
                        .unwrap_or_default()
                }}
            </p>
        </Show>
    }
}

#[cfg(test)]
mod day49_control_navigation_tests {
    use super::*;
    #[test]
    fn navigation_reaches_exactly_six_real_sections_without_commands() {
        Owner::new().with(|| {
            let html = view! { <ControlNavigation /> }.to_html();
            for (id, title) in [
                ("pet-interaction", "互動"),
                ("pet-settings", "設定"),
                ("pet-memory", "記憶"),
                ("pet-files", "檔案"),
                ("pet-management", "管理"),
                ("pet-updates", "更新"),
            ] {
                assert!(html.contains(&format!("href=\"#{id}\"")));
                assert!(html.contains(title));
            }
            assert_eq!(html.matches("href=\"#pet-").count(), 6);
            assert!(!html.contains("<button"));
            assert!(!html.contains("javascript:"));
        });
    }
}

#[cfg(test)]
mod day50_activity_target_tests {
    use super::*;

    #[test]
    fn only_selected_existing_pet_can_use_interaction_controls() {
        let rows = vec![
            SavedPet {
                id: "a".into(),
                name: "第一隻".into(),
                species: "cat".into(),
            },
            SavedPet {
                id: "b".into(),
                name: "第二隻".into(),
                species: "dog".into(),
            },
        ];
        assert_eq!(
            activity_target::activity_target(&rows, Some("b")),
            Some("b".into())
        );
        assert_eq!(
            activity_target::activity_target(&rows, Some("missing")),
            None
        );
        assert_eq!(activity_target::activity_target(&rows, None), None);
        assert_eq!(rows.len(), 2); // 選擇管理對象不能修改寵物資料。
    }
}
