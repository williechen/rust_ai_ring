mod bridge;
mod cage_status;
mod component;
mod duplicate_preview;
mod file_scan_view;

pub use component::*;

use leptos::mount::mount_to_body;
use leptos::{prelude::*, task::spawn_local};
use pet_domain::PetSnapshot;
use std::collections::HashMap;

use crate::cage_status::record_cage_status;

fn accept_scan_result(request_generation: u64, current_generation: u64) -> bool {
    request_generation == current_generation
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

    let (duplicate_status, set_duplicate_status) = signal::<Option<String>>(None);
    let (duplicate_loading, set_duplicate_loading) = signal(false);

    let (scan_loading, set_scan_loading) = signal(false);
    let (scan_status, set_scan_status) = signal::<Option<String>>(None);

    // 放在 App 的 signals 區塊；確認只針對目前顯示的掃描結果。
    let (confirmed, set_confirmed) = signal(false);
    let (generation, set_generation) = signal(0_u64);

    let (new_pet_name, set_new_pet_name) = signal(String::new());
    let (new_pet_feedback, set_new_pet_feedback) = signal(String::new());

    let (saved_pets, set_saved_pets) = signal(Vec::<bridge::SavedPet>::new());
    let (pets_busy, set_pets_busy) = signal(false);
    let cage_status = RwSignal::new(HashMap::<String, String>::new());

    let (active_pet, set_active_pet) = signal::<Option<bridge::SavedPet>>(None);
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

    view! {
        <main class="control-shell">
            <h1>"寵物控制台"</h1>
            <p>"控制台關閉後寵物仍運作；檔案功能目前只提供唯讀預覽"</p>

            <section>
                <h2>"新增寵物"</h2>
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
                                        Ok(rows) => set_saved_pets.set(rows),
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
                                    set_saved_pets.set(rows);
                                    set_new_pet_feedback.set("已重新載入清單".into());
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
                            view! { <li>{format!("{}／{}（{}）", pet.name, pet.species, pet.id)}
                                <button type="button" disabled=move || pets_busy.get()
                                    on:click=move |_| {
                                        if pets_busy.get_untracked() { return; }
                                        set_pets_busy.set(true); let id=id.clone();
                                        spawn_local(async move {
                                            match bridge::select_pet(&id).await {
                                                Ok(pet) => {
                                                    cage_status.update(|statuses| {
                                                        statuses.remove(&id);
                                                    });
                                                    set_active_pet.set(Some(pet));
                                                    set_new_pet_feedback.set("生活窗選取已儲存".into());
                                                },
                                                Err(message) => set_new_pet_feedback.set(message),
                                            }
                                            set_pets_busy.set(false);
                                        });
                                    }>"選為生活窗寵物"</button>
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

            <h2>"互動與聊天"</h2>
            <p>{move || snapshot.get().map(|s| format!("目前狀態: {:?}, 最近更新: {:?}", s.state, s.revision)).unwrap_or_else(|| "讀取中…".into())}</p>

            <PetControls
                snapshot
                set_snapshot
                set_error
            />

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

            <h2>"檔案管理"</h2>
            <button
                type="button"
                disabled=move || scan_loading.get() || duplicate_loading.get()
                on:click=move |_| {
                    if scan_loading.get_untracked() || duplicate_loading.get_untracked() { return; }
                    let request = generation.get_untracked().wrapping_add(1);
                    set_generation.set(request);
                    set_scan_loading.set(true);
                    set_scan_status.set(None);
                    set_error.set(None);
                    // 新掃描一律使前一份確認失效，也不保留另一類舊摘要。
                    set_confirmed.set(false);
                    set_scan_status.set(None);
                    set_duplicate_status.set(None);
                    spawn_local(async move {
                        let result = bridge::scan_file_preview().await;
                        set_scan_loading.set(false);
                        if !accept_scan_result(request, generation.get_untracked()) {
                            return;
                        }
                        match result {
                            Ok(summary) => set_scan_status.set(Some(summary)),
                            // 不把平台錯誤中的私人路徑顯示於畫面。
                            Err(_) => set_error.set(Some("掃描失敗，請確認已授權目錄後重試。".into())),
                        }
                        set_scan_loading.set(false);
                    });
                }
            >"預覽授權目錄"</button>
            <Show when=move || scan_loading.get() fallback=|| ()>
                <p role="status">"正在掃描授權目錄…"</p>
            </Show>
            <Show when=move || scan_status.get().is_some() fallback=|| ()>
                <p role="status">{move || scan_status.get().unwrap_or_default()}</p>
            </Show>

            <button
                type="button"
                disabled=move || duplicate_loading.get() || scan_loading.get()
                on:click=move |_| {
                    // 清除過時預覽；同時只允許一次使用者請求。
                    if duplicate_loading.get_untracked() || scan_loading.get_untracked()  { return; }
                    let request = generation.get_untracked().wrapping_add(1);
                    set_generation.set(request);
                    set_duplicate_loading.set(true);
                    set_duplicate_status.set(None);
                    set_error.set(None);
                    // 新掃描一律使前一份確認失效，也不保留另一類舊摘要。
                    set_confirmed.set(false);
                    set_scan_status.set(None);
                    set_duplicate_status.set(None);
                    spawn_local(async move {
                        let result = bridge::scan_duplicate_preview().await;
                        set_duplicate_loading.set(false);
                        if !accept_scan_result(request, generation.get_untracked()) {
                            return;
                        }
                        match result {
                            Ok(summary) => set_duplicate_status.set(Some(summary)),
                            Err(_) => set_error.set(Some("預覽失敗，請確認授權目錄後重試。".into())),
                        }
                        set_duplicate_loading.set(false);
                    });
                }
            >"檢查重複檔案"</button>
            <Show when=move || duplicate_loading.get() fallback=|| ()>
                <p role="status">"正在檢查重複檔案…"</p>
            </Show>
            <Show when=move || duplicate_status.get().is_some() fallback=|| ()>
                <pre class="duplicate-preview" role="status">
                    {move || duplicate_status.get().unwrap_or_default()}
                </pre>
            </Show>

            <button type="button"
                disabled=move || scan_loading.get() || duplicate_loading.get()
                    || (scan_status.get().is_none() && duplicate_status.get().is_none())
                on:click=move |_| set_confirmed.set(true)
            >"確認本次預覽"</button>
            <button type="button"
                disabled=move || scan_loading.get() || duplicate_loading.get()
                on:click=move |_| {
                    set_confirmed.set(false);
                    set_scan_status.set(None);
                    set_duplicate_status.set(None);
                }
            >"取消／清除本次預覽"</button>
            <Show when=move || confirmed.get() fallback=|| ()>
                <p role="status">"已確認本次預覽；尚未執行移動、刪除或還原。"</p>
            </Show>

            <ErrorMessage error />

            <button
                type="button"
                on:click=move |_| spawn_local(async move {
                    let _ = bridge::quit_pet().await;
                })
            >"寵物回寵了"</button>
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
mod day16_scan_generation_tests {
    use super::accept_scan_result;

    #[test]
    fn canceled_or_superseded_scan_cannot_restore_preview() {
        assert!(accept_scan_result(3, 3));
        assert!(!accept_scan_result(2, 3));
        assert!(!accept_scan_result(3, 4));
    }
}
