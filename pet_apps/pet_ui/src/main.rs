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
pub mod management_datatable;
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

#[derive(Clone)]
enum SettingsDestination {
    Page(ControlPage),
    Pet(Option<String>),
}

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

    let (new_pet_name, set_new_pet_name) = signal(String::new());
    let (new_pet_feedback, set_new_pet_feedback) = signal(String::new());
    let (saved_pets, set_saved_pets) = signal(Vec::<SavedPet>::new());
    let (pets_busy, set_pets_busy) = signal(false);

    let table_query = RwSignal::new(String::new());
    let table_size = RwSignal::new(10usize);
    let table_page = RwSignal::new(1usize);
    let table_sort = RwSignal::new(management_datatable::Sort::default());
    let rename_drafts = RwSignal::new(HashMap::<String, String>::new());
    let table_view = Memo::new(move |_| {
        management_datatable::project(
            &saved_pets.get(),
            &table_query.get(),
            table_sort.get(),
            table_page.get(),
            table_size.get(),
        )
    });
    Effect::new(move |_| {
        let rows = saved_pets.get();
        rename_drafts.update(|drafts| drafts.retain(|id, _| rows.iter().any(|pet| &pet.id == id)));
    });
    let sort_pets = move |field| {
        table_sort.update(|sort| sort.toggle(field));
        table_page.set(1);
    };

    let (active_pet, set_active_pet) = signal::<Option<SavedPet>>(None);

    let (file_directory, set_file_directory) = signal(None::<Vec<String>>);
    let (management_pet_id, set_management_pet_id) = signal(None::<String>);

    let cage_status = RwSignal::new(HashMap::<String, String>::new());
    let startup_failures = RwSignal::new(HashMap::<String, StartupRestoreFailure>::new());
    let profile_events = RwSignal::new(0_u64);

    spawn_local(async move {
        if let Err(message) = bridge::listen_pet_profile(move |pet| {
            profile_events.update(|n| *n += 1);

            let generation = profile_events.get_untracked();
            let id = pet.id.clone();

            set_snapshot.set(None);
            set_active_pet.set(Some(pet));
            set_error.set(None);

            spawn_local(async move {
                let result = bridge::get_pet_state_for(Some(id.clone())).await;

                if profile_events.get_untracked() != generation
                    || !active_pet
                        .with_untracked(|pet| pet.as_ref().is_some_and(|pet| pet.id == id))
                {
                    return;
                }
                match result {
                    Ok(incoming) => apply_snapshot(incoming, snapshot, set_snapshot),
                    Err(message) => set_error.set(Some(message)),
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

        let generation = profile_events.get_untracked();
        spawn_local(async move {
            let result = bridge::get_pet_state().await;
            if profile_events.get_untracked() != generation {
                return;
            }
            match result {
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
                <button type="button" 
                    on:click=move |_| spawn_local(async move {
                        if let Err(e)=bridge::cage_saved_pet().await {
                            set_error.set(Some(e));
                        }
                    })
                >"回籠子"</button>
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
    let settings_dirty = RwSignal::new(false);
    let settings_busy = RwSignal::new(false);
    let settings_save = RwSignal::new(0u64);
    let settings_discard = RwSignal::new(0u64);
    let pending = RwSignal::new(None::<SettingsDestination>);

    let apply_navigation = Callback::new(move |target: SettingsDestination| match target {
        SettingsDestination::Page(next) => {
            page.set(next);
            next.focus_tab();
        }
        SettingsDestination::Pet(id) => set_management_pet_id.set(id),
    });

    let request_navigation = Callback::new(move |target: SettingsDestination| {
        if settings_busy.get_untracked() || pending.get_untracked().is_some() {
            return;
        }

        let same = match &target {
            SettingsDestination::Page(next) => *next == page.get_untracked(),
            SettingsDestination::Pet(id) => *id == management_pet_id.get_untracked(),
        };

        if same {
            return;
        }

        if settings_dirty.get_untracked() {
            pending.set(Some(target));
        } else {
            apply_navigation.run(target);
        }
    });

    let (startup_view, set_startup_view) = signal(false);

    view! {
        <main class="control-shell">
            <style>{include_str!("css/control.css")}</style>
           <header class="control-heading">
                <span class="control-mark" 
                    aria-hidden="true">"P"</span>
                <h1>"Pet｜桌面陪伴"</h1>
            </header>
            <ControlNavigation page 
                on_navigate=Callback::new(move |next|request_navigation.run(SettingsDestination::Page(next))) />
            <div class="control-context" 
                hidden=move || !matches!(page.get(), ControlPage::Interaction | ControlPage::Settings | ControlPage::Memory)>
                <label class="control-pet-picker">
                    <span>"目前寵物"</span>
                    <select disabled=move || pets_busy.get() || settings_busy.get() || pending.get().is_some() || saved_pets.with(Vec::is_empty)
                        prop:value=move || management_pet_id.get().unwrap_or_default()
                        on:change=move |event| {
                            let id = event_target_value(&event);
                            let target=if id.is_empty(){None}else{Some(id)};
                            // 受控選單先還原已提交的對象，確認成功後才更新。
                            event_target::<web_sys::HtmlSelectElement>(&event).set_value(&management_pet_id.get_untracked().unwrap_or_default());
                            if target.as_ref().is_none_or(|id| saved_pets.get_untracked().iter().any(|pet| &pet.id==id)) {
                                request_navigation.run(SettingsDestination::Pet(target));
                            }
                        }>
                        <option value="">"選擇寵物"</option>
                        <For each=move || saved_pets.get() 
                            key=|pet| (pet.id.clone(), pet.name.clone())
                            children=move |pet| {
                                let id = pet.id.clone();
                                view! {
                                    <option value=pet.id 
                                        prop:selected=move || management_pet_id.with(|selected| selected.as_ref() == Some(&id))
                                    >{pet.name}</option>
                                }
                            } />
                    </select>
                </label>
                <p class="control-context-note">"選擇對象只會切換管理畫面。"</p>
                <Show when=move || pending.get().is_some()>
                    <section class="settings-navigation-confirmation" 
                        role="alertdialog" 
                        aria-modal="false" 
                        aria-labelledby="settings-navigation-title">
                        <h2 id="settings-navigation-title">"設定尚未儲存"</h2>
                        <p>"切換前請儲存設定、捨棄草稿，或留在目前畫面。儲存失敗會保留草稿與原對象。"</p>
                        <button type="button" 
                            disabled=move || settings_busy.get() 
                            on:click=move |_| pending.set(None)
                        >"留在此頁"</button>
                        <button type="button" 
                            disabled=move || settings_busy.get() 
                            on:click=move |_| {
                                settings_discard.update(|n|*n+=1);
                                settings_dirty.set(false);
                                if let Some(target)=pending.get_untracked(){
                                    pending.set(None);
                                    apply_navigation.run(target);
                                }
                            }
                        >"捨棄並切換"</button>
                        <button type="button" 
                            disabled=move || settings_busy.get() 
                            on:click=move |_| settings_save.update(|n|*n+=1)
                        >"儲存後切換"</button>
                    </section>
                </Show>
            </div>

            <section id="pet-interaction" 
                role="tabpanel" 
                aria-labelledby="pet-interaction-tab" 
                tabindex="0"
                hidden=move || page.get() != ControlPage::Interaction>
                <div class="control-section-heading">
                    <div>
                        <h2 id="pet-interaction-title">"陪伴與互動"</h2>
                        <p>"切換活動只會影響所選寵物。"</p>
                    </div>
                </div>

                <For each=move || {activity_target::activity_target(&saved_pets.get(), management_pet_id.get().as_deref()).into_iter().collect::<Vec<_>>()} 
                    key=String::clone
                    children=move |id| view! {{
                        let display_id = id.clone();
                        let pet = Signal::derive(move || saved_pets.with(|pets| pets.iter().find(|row| row.id == display_id).cloned()));
                        view! { <control_activity::ActivityWorkspace id pet busy=pets_busy set_busy=set_pets_busy page /> }
                    }} />
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
                    <PetControls active_pet profile_events snapshot set_snapshot set_error />
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

            <section id="pet-settings" 
                role="tabpanel" 
                aria-labelledby="pet-settings-tab" 
                tabindex="0"
                hidden=move || page.get() != ControlPage::Settings>

                <h2 id="pet-settings-title">
                    {move || saved_pets.with(|rows| {
                        let id = management_pet_id.get();
                        rows.iter().find(|pet| Some(&pet.id) == id.as_ref()).map(|pet| format!("{}的設定", pet.name)).unwrap_or_else(|| "寵物設定".into())
                    })}
                </h2>

                <p class="control-context-note">"編輯這隻寵物的資料，儲存前不會寫入。"</p>

                <div class="settings-jump" 
                    role="group" 
                    aria-label="設定分類">
                    <button type="button" 
                        aria-pressed=move || (!startup_view.get()).to_string()
                        aria-controls="pet-settings-profile" 
                        on:click=move |_| set_startup_view.set(false)
                    >"資料與外觀"</button>
                    <button type="button" 
                        aria-pressed=move || startup_view.get().to_string()
                        aria-controls="pet-startup" 
                        on:click=move |_| set_startup_view.set(true)
                    >"啟動預設"</button>
                </div>

                <div id="pet-settings-profile" 
                    hidden=move || startup_view.get()>
                    <settings_editor::PetSettingsEditor
                        selected_id=management_pet_id
                        dirty=settings_dirty
                        busy=settings_busy
                        save_request=settings_save
                        discard_request=settings_discard
                        navigation_pending=Signal::derive(move || pending.get().is_some())
                        on_saved=Callback::new(move |(id,p):(String,pet_domain::profile::PetProfile)|{
                            set_saved_pets.update(|rows|{
                                if let Some(row)=rows.iter_mut().find(|r|r.id==id){
                                    row.name=p.name.clone();row.species=p.species.clone();
                                }
                            });
                            if active_pet.get_untracked().as_ref().is_some_and(|pet|pet.id==id){
                                set_active_pet.set(Some(SavedPet{id,name:p.name,species:p.species}));
                            }
                            settings_dirty.set(false);
                            if let Some(target)=pending.get_untracked(){
                                pending.set(None);apply_navigation.run(target);
                            }
                        })
                    />
                </div>
                <section id="pet-startup" 
                    hidden=move || !startup_view.get()>
                    <h3>"啟動預設"</h3>
                    <p class="control-context-note">"啟動預設與目前出籠狀態分開。"</p>
                    <control_activity::StartupPreferences selected=management_pet_id />
                </section>
            </section>

            <section id="pet-memory" 
                role="tabpanel" 
                aria-labelledby="pet-memory-tab" 
                tabindex="0"
                hidden=move || page.get() != ControlPage::Memory>
                <h2 id="pet-memory-title">"記憶"</h2>
                <p>"記憶查詢與管理尚未接通。"</p>
            </section>

            <section id="pet-files" 
                role="tabpanel" 
                aria-labelledby="pet-files-tab" 
                tabindex="0"
                hidden=move || page.get() != ControlPage::Files>
                <h2 id="pet-files-title">"檔案管理"</h2>
                <p class="control-context-note">"瀏覽本機已授權的目錄。"</p>
                <file_browser::FileBrowser 
                    active=Signal::derive(move || page.get() == ControlPage::Files)
                    on_directory=Callback::new(move |directory| set_file_directory.set(directory)) />
                <fieldset class="file-operation-layout" 
                    disabled 
                    aria-describedby="file-operations-status">
                    <legend>"檔案操作"</legend>
                    <p id="file-operations-status">"按鈕位置已就緒，待接線。"</p>
                    <button type="button">"開啟"</button>
                    <button type="button">"複製"</button>
                    <button type="button">"移動"</button>
                    <button type="button">"移至垃圾桶"</button>
                </fieldset>
                <file_comparison::FileComparison directory=file_directory />
            </section>

            <section id="pet-management" 
                role="tabpanel" 
                aria-labelledby="pet-management-tab" 
                tabindex="0"
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
                        spawn_local(async move {
                             match pet_application::creation::create_caged(&bridge::CreationBridge, proposed).await {
                                Ok(result) => {
                                    let created = result.pet;
                                    let (rows, refreshed) = match result.listing {
                                        Ok(rows) => (rows, true),
                                        Err(_) => {
                                            let mut rows = saved_pets.get_untracked();
                                            // 讀回失敗仍保留已提交身分，不重送建立。
                                            if !rows.iter().any(|row| row.id == created.id) {
                                                rows.push(created.clone());
                                            }
                                            (rows, false)
                                        }
                                    };
                                    management_selection::reconcile_management_pet(
                                        &rows, management_pet_id, set_management_pet_id,
                                    );
                                    let present = rows.iter().any(|row| row.id == created.id);
                                    set_saved_pets.set(rows);
                                    if present {
                                        let _ = management_selection::choose_management_pet(
                                            &created.id, &saved_pets.get_untracked(), set_management_pet_id,
                                        );
                                    }
                                    set_new_pet_name.set(String::new());
                                    set_new_pet_feedback.set(format!(
                                        "已建立：{}（{}）；目前維持回籠。{}",
                                        created.name, created.id,
                                        if !refreshed { "清單讀回失敗，請重新載入，勿重送建立。" } else if !present { "清單已無此寵物，請重新選擇。" } else { "可從互動區出籠。" }
                                    ));
                                }
                                Err(error) => set_new_pet_feedback.set(format!("建立寵物失敗：{error}")),
                            }
                            set_pets_busy.set(false);
                        });
                    }
                >"建立寵物"</button>
                <p role="status">{move || new_pet_feedback.get()}</p>
                <Show when=move || pets_busy.get() fallback=|| ()><p>"處理中…"</p></Show>
                <div class="management-datatable" aria-busy=move ||pets_busy.get().to_string()>
                    <div class="management-table-toolbar">
                        <div class="management-query-row">
                            <label for="management-table-search">"搜尋寵物"
                                <input id="management-table-search" 
                                    type="search" 
                                    placeholder="名稱、種類或識別碼" 
                                    prop:value=move ||table_query.get()
                                    on:input=move |event|{
                                        table_query.set(event_target_value(&event));
                                        table_page.set(1);
                                    }/>
                            </label>
                            <button type="button" disabled=move || pets_busy.get()
                                on:click=move |_| {
                                    if pets_busy.get_untracked() { return; }
                                    set_pets_busy.set(true);
                                    spawn_local(async move {
                                        match bridge::list_pets().await {
                                            Ok(rows) => {
                                                // 只在已提交的新清單中確認管理對象是否仍存在。
                                                let removed = management_selection::reconcile_management_pet(&rows, management_pet_id, set_management_pet_id);
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
                                }
                            >"重新載入清單"</button>
                        </div>
                        <Show when=move ||!table_query.get().is_empty()>
                            <button type="button" 
                                on:click=move |_|{
                                    table_query.set(String::new());
                                    table_page.set(1);
                                }
                            >"清除搜尋"</button>
                        </Show>
                        <label for="management-table-size">"每頁筆數"
                            <select id="management-table-size" 
                                prop:value=move ||table_size.get().to_string() 
                                on:change=move |event|{
                                    let n=event_target_value(&event).parse::<usize>().unwrap_or(10);
                                    table_size.set(if [10,25,50].contains(&n){n}else{10});
                                    table_page.set(1);
                                }
                            >
                                <option value="10">"10"</option>
                                <option value="25">"25"</option>
                                <option value="50">"50"</option>
                            </select>
                        </label>
                    </div>
                    <p class="management-table-info" role="status">
                        {move ||{
                            let v=table_view.get();
                            format!("顯示 {}–{} 筆，共 {} 筆符合；全部 {} 隻",v.start,v.end,v.filtered,saved_pets.with(Vec::len))
                        }}
                    </p>
                    <div class="management-table-scroll" 
                        role="region" 
                        aria-label="寵物管理資料表" 
                        tabindex="0">
                        <table class="management-data-table">
                            <caption>"已儲存寵物管理清單"</caption>
                            <thead>
                                <tr>
                                    <th scope="col" 
                                        aria-sort=move ||table_sort.get().aria(management_datatable::Column::Name)>
                                        <button type="button" 
                                            on:click=move |_|sort_pets(management_datatable::Column::Name)
                                        >"名稱"</button>
                                    </th>
                                    <th scope="col" 
                                        aria-sort=move ||table_sort.get().aria(management_datatable::Column::Species)>
                                        <button type="button" 
                                            on:click=move |_|sort_pets(management_datatable::Column::Species)
                                        >"種類"</button>
                                    </th>
                                    <th scope="col" 
                                        aria-sort=move ||table_sort.get().aria(management_datatable::Column::Id)>
                                        <button type="button" 
                                            on:click=move |_|sort_pets(management_datatable::Column::Id)
                                        >"識別碼"</button>
                                    </th>
                                    <th scope="col">"管理狀態"</th>
                                    <th scope="col">"操作"</th>
                                </tr>
                            </thead>
                            <tbody>
                                <For each=move || table_view.get().rows 
                                    key=|pet| pet.id.clone()
                                    children=move |pet| {

                                        let id=pet.id.clone();
                                        let open_id=id.clone();
                                        let cage_id=id.clone();
                                        let find_id=id.clone();
                                        let status_id=id.clone();
                                        let display_id=id.clone();
                                        let selected_id=id.clone();
                                        let rename_scope=id.clone();
                                        let (rename_input, set_rename_input) = signal(rename_drafts.with_untracked(|drafts|drafts.get(&pet.id).cloned()).unwrap_or_else(||pet.name.clone()));
                                        let draft_id=StoredValue::new(pet.id.clone());
                                        let name_id=pet.id.clone();
                                        let species_id=pet.id.clone();
                                        let row_id=pet.id.clone();
                                        let (rename_id, set_rename_id) = signal(pet.id.clone());
                                        view! { 
                                            <tr data-pet-id=row_id.clone() class:management-selected=move ||management_pet_id.get().as_deref()==Some(row_id.as_str())>
                                                <td class="management-pet-name">
                                                    {move ||saved_pets.with(|rows|rows.iter().find(|row|row.id==name_id).map(|row|row.name.clone()).unwrap_or_default())}
                                                </td>
                                                <td>
                                                    {move ||saved_pets.with(|rows|rows.iter().find(|row|row.id==species_id).map(|row|row.species.clone()).unwrap_or_default())}
                                                </td>
                                                <td>
                                                    <code>{pet.id.clone()}</code>
                                                </td>
                                                <td>
                                                    <span role="status">
                                                        {move || if management_pet_id.get().as_deref()==Some(selected_id.as_str()) { "目前管理對象" } else { "未選取" }}
                                                    </span>
                                                </td>
                                                <td>
                                                    <details class="management-row-actions">
                                                        <summary>"操作"</summary>
                                                        <div>
                                                            <Show when=move || management_pet_id.get().as_deref()==Some(rename_scope.as_str()) fallback=|| ()>
                                                                <label>"修改名稱"
                                                                    <input aria-label="寵物新名稱" 
                                                                        maxlength="64"
                                                                        prop:value=move || rename_input.get()
                                                                        on:input=move |event| {
                                                                            let value=event_target_value(&event);
                                                                            set_rename_input.set(value.clone());
                                                                            rename_drafts.update(|drafts|{drafts.insert(draft_id.get_value(),value);});
                                                                        } 
                                                                    />
                                                                </label>
                                                                <button type="button" 
                                                                    disabled=move || pets_busy.get()
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
                                                                                    rename_drafts.update(|drafts|{if drafts.get(&id)==Some(&proposed){drafts.remove(&id);}});
                                                                                    set_new_pet_feedback.set("已更新寵物名稱".into());
                                                                                }
                                                                                Err(error) => set_new_pet_feedback.set(format!("更名失敗：{error}")),
                                                                            }
                                                                            set_pets_busy.set(false);
                                                                        });
                                                                    }
                                                                >"儲存名稱"</button>
                                                            </Show>
                                                            <button type="button" 
                                                                disabled=move || pets_busy.get()
                                                                on:click=move |_| {
                                                                    match management_selection::choose_management_pet(&id,&saved_pets.get_untracked(),set_management_pet_id) {
                                                                        Ok(()) => set_new_pet_feedback.set("已選為管理對象；出入籠狀態保持不變".into()),
                                                                        Err(message) => set_new_pet_feedback.set(message),
                                                                    }
                                                                }
                                                            >"選為管理對象"</button>
                                                            <button type="button" 
                                                                disabled=move || pets_busy.get()
                                                                on:click=move |_| {
                                                                    if pets_busy.get_untracked() {return;}
                                                                    set_pets_busy.set(true);
                                                                    let id=open_id.clone();
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
                                                                }
                                                            >"出籠"</button>
                                                            <button type="button" 
                                                                disabled=move || pets_busy.get()
                                                                on:click=move |_| {
                                                                    if pets_busy.get_untracked() { return; }
                                                                    set_pets_busy.set(true);
                                                                    let id = status_id.clone();
                                                                    spawn_local(async move {
                                                                        let result = bridge::pet_cage_status_by_id(&id).await;
                                                                        cage_status.update(|statuses| record_cage_status(statuses, &id, result));
                                                                        set_pets_busy.set(false);
                                                                    });
                                                                }
                                                            >"查詢回籠狀態"</button>
                                                            <p role="status" 
                                                                aria-live="polite">
                                                                {move || cage_status.get().get(&display_id).cloned().unwrap_or_default()}
                                                            </p>
                                                            <button type="button" 
                                                                disabled=move || pets_busy.get()
                                                                on:click=move |_| {
                                                                    if pets_busy.get_untracked() { return; }
                                                                    set_pets_busy.set(true);
                                                                    let id = cage_id.clone();
                                                                    spawn_local(async move {
                                                                        match bridge::cage_saved_pet_by_id(&id).await {
                                                                            Ok(()) => {
                                                                                cage_status.update(|statuses| { statuses.remove(&id); });
                                                                                if active_pet.get_untracked().as_ref().is_some_and(|pet| pet.id == id) {
                                                                                    set_active_pet.set(None);
                                                                                    set_snapshot.set(None);
                                                                                }
                                                                                set_new_pet_feedback.set("該隻已回籠；其他寵物保持原狀".into());
                                                                            }
                                                                            Err(error) => set_new_pet_feedback.set(format!("回籠失敗：{error}")),
                                                                        }
                                                                        set_pets_busy.set(false);
                                                                    });
                                                                }
                                                            >"回籠這隻"</button>
                                                            <button type="button" 
                                                                disabled=move || pets_busy.get()
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
                                                                }
                                                            >"找回寵物"</button>
                                                        </div>
                                                    </details>
                                                </td>
                                            </tr> 
                                        }
                                    } />
                                    <Show when=move ||table_view.with(|v|v.rows.is_empty())>
                                        <tr>
                                            <td colspan="5">
                                                {move ||if table_query.get().trim().is_empty(){"目前沒有可顯示的寵物；可重新載入清單。"}else{"沒有符合的寵物，請調整或清除搜尋。"}}
                                            </td>
                                        </tr>
                                    </Show>
                            </tbody>
                        </table>
                    </div>
                    <nav class="management-table-pagination" aria-label="寵物表格分頁">
                        <button type="button" 
                            disabled=move ||table_view.with(|v|v.page<=1) 
                            on:click=move |_|table_page.set(table_view.get_untracked().page.saturating_sub(1).max(1))
                        >"上一頁"</button>
                        <span>{move ||{let v=table_view.get();format!("第 {} / {} 頁",v.page,v.pages)}}</span>
                        <button type="button" 
                            disabled=move ||table_view.with(|v|v.page>=v.pages) 
                            on:click=move |_|{let v=table_view.get_untracked();table_page.set((v.page+1).min(v.pages));}
                        >"下一頁"</button>
                    </nav>
                </div>
            </section>

            <section id="pet-updates" 
                role="tabpanel" 
                aria-labelledby="pet-updates-tab" 
                tabindex="0"
                hidden=move || page.get() != ControlPage::Updates>
                <h2 id="pet-updates-title">"更新"</h2>
                <p>"版本檢查與簽章更新尚未接通。"</p>
            </section>

            <footer class="control-footer">
                <span>"關閉控制台，陪伴繼續"</span>
                <button type="button" 
                    on:click=move |_| spawn_local(async move {
                        let _ = bridge::quit_pet().await;
                    })
                >"結束寵物程式"</button>
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
