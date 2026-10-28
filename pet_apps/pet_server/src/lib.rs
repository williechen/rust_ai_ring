mod chat;
mod file_organizer;
pub mod pet_store;
pub mod pet_windows;
mod roam;
use pet_application::normalize_new_pet_name;

use ai_core::{ChatRequest, MockChatModel};
use pet_domain::{PetCommand, PetMachine, PetSnapshot};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};

use crate::chat::AiState;
use crate::roam::roam_enabled_value;

// 派發寵物指令並更新狀態
fn dispatch_pet_command(
    app: &tauri::AppHandle,
    pet: &Mutex<PetMachine>,
    command: PetCommand,
) -> Result<PetSnapshot, String> {
    let snapshot = {
        let mut pet = pet
            .lock()
            .map_err(|_| "pet state lock poisoned".to_string())?;

        pet.dispatch(command).map_err(|error| error.to_string())?
    };

    app.emit("pet://state-changed", snapshot)
        .map_err(|error| error.to_string())?;

    Ok(snapshot)
}

fn authorized_root() -> Result<file_organizer::AuthorizedRoot, String> {
    let path =
        std::env::var("PET_AUTHORIZED_ROOT").map_err(|_| "尚未設定授權掃描目錄".to_string())?;
    file_organizer::AuthorizedRoot::new(path).map_err(|e| format!("無法使用授權掃描目錄：{e}"))
}

fn roam_enabled() -> bool {
    let value = std::env::var("PET_ENABLE_ROAM").ok();
    roam_enabled_value(value.as_deref())
}

fn control_menu_pet_id(menu_id: &str) -> Option<String> {
    let id = menu_id
        .strip_prefix("pet:")?
        .strip_suffix(":open_control")?;
    uuid::Uuid::parse_str(id).ok().map(|id| id.to_string())
}

// 取得寵物狀態
#[tauri::command]
fn get_pet_state(pet: State<'_, Mutex<PetMachine>>) -> Result<PetSnapshot, String> {
    let pet = pet.lock().map_err(|_| "pet state unavailable")?;
    Ok(pet.snapshot())
}

// 寵物指令
#[tauri::command]
fn send_pet_command(
    app: tauri::AppHandle,
    pet: State<'_, Mutex<PetMachine>>,
    command: PetCommand,
) -> Result<PetSnapshot, String> {
    dispatch_pet_command(&app, pet.inner(), command)
}

// 與寵物進行聊天
#[tauri::command]
async fn chat_with_pet(
    message: String,
    state: tauri::State<'_, AiState>,
) -> Result<String, String> {
    let response = state
        .model
        .chat(ChatRequest::new(message))
        .await
        .map_err(|error| error.to_string())?;
    Ok(response.text)
}

#[tauri::command]
fn scan_file_preview() -> Result<file_organizer::ScanPreview, String> {
    let root = authorized_root()?;
    file_organizer::scan_preview(&root).map_err(|e| format!("掃描失敗：{e}"))
}

#[tauri::command]
fn scan_duplicate_preview() -> Result<file_organizer::DuplicatePreview, String> {
    let root = authorized_root()?;
    let scan = file_organizer::scan(&root).map_err(|e| format!("掃描失敗：{e}"))?;
    // 僅回傳 SHA256 分組與警告；不提供刪除或 Trash 能力。
    Ok(file_organizer::duplicate_preview(&scan))
}

#[tauri::command]
fn open_control_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("控制視窗不存在")?;
    window.show().map_err(|_| "無法顯示控制視窗")?;
    window.set_focus().map_err(|_| "無法聚焦控制視窗".into())
}

#[tauri::command]
fn quit_pet(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn validate_new_pet_name(name: String) -> Result<String, String> {
    normalize_new_pet_name(&name)
}

#[tauri::command]
async fn create_pet(
    name: String,
    store: State<'_, pet_store::PetStore>,
) -> Result<pet_store::SavedPet, String> {
    store.create(&name).await
}

#[tauri::command]
async fn pet_cage_status_by_id(
    id: String,
    store: State<'_, pet_store::PetStore>,
) -> Result<bool, String> {
    store.read_caged(&id).await
}

#[tauri::command]
async fn list_pets(
    store: State<'_, pet_store::PetStore>,
) -> Result<Vec<pet_store::SavedPet>, String> {
    store.list().await
}

#[tauri::command]
async fn active_pet(
    store: State<'_, pet_store::PetStore>,
) -> Result<Option<pet_store::SavedPet>, String> {
    store.active().await
}

#[tauri::command]
async fn select_pet(
    id: String,
    app: tauri::AppHandle,
    store: State<'_, pet_store::PetStore>,
) -> Result<pet_store::SavedPet, String> {
    let _operation = pet_windows::lifecycle_guard().await;
    let saved = store.select(&id).await?;
    pet_windows::open(&app, &saved)?;
    app.emit("pet-profile-changed", &saved)
        .map_err(|_| "選取已儲存，通知失敗，請重新載入生活窗".to_owned())?;
    Ok(saved)
}

pub fn application(db: sqlx::SqlitePool) {
    tauri::Builder::default()
        .on_window_event(|window, event| {
            // 控制台關閉僅隱藏，讓既有 show/focus IPC 能恢復同一個 main。
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Err(error) = window.hide() {
                        // 隱藏失敗時仍保留視窗；不把錯誤當成銷毀成功。
                        eprintln!("隱藏控制視窗失敗：{error}");
                    }
                }
                return;
            }

            // Native X/close is the same cage transaction as the explicit IPC command.
            // The control window keeps its existing hide-on-close behavior above.
            if let Some(id) = pet_windows::pet_id_from_window_label(window.label()) {
                match event {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let window = window.clone();
                        let store = window
                            .app_handle()
                            .state::<pet_store::PetStore>()
                            .inner()
                            .clone();
                        tauri::async_runtime::spawn(async move {
                            let _operation = pet_windows::lifecycle_guard().await;
                            if let Err(error) = pet_application::lifecycle::persist_then_close(
                                |flag| store.set_caged(&id, flag),
                                || window.destroy().map_err(|e| e.to_string()),
                            )
                            .await
                            {
                                // DB failure must leave the window alive; never silently claim cage.
                                eprintln!("生活窗直接關閉／回籠失敗：{error}");
                            }
                        });
                    }
                    tauri::WindowEvent::Destroyed => {
                        window
                            .app_handle()
                            .state::<pet_windows::Runtime>()
                            .remove(&id);
                    }
                    _ => {}
                }
            }
        })
        .manage(Mutex::new(PetMachine::default()))
        .manage(pet_windows::Runtime::default())
        .manage(AiState::new(Arc::new(MockChatModel)))
        .manage(pet_store::PetStore { pool: db })
        .setup(|app| {
            let handle = app.handle().clone();
            let store = app.state::<pet_store::PetStore>().inner().clone();
            tauri::async_runtime::spawn(async move {
                let _operation = pet_windows::lifecycle_guard().await;
                match store.uncaged().await {
                    Ok(pets) => {
                        for pet in pets {
                            // 每個 UUID 由既有 open() 建立獨立原生視窗與 Runtime。
                            // 單隻失敗不應讓其他未回籠寵物無法恢復。
                            if let Err(error) = pet_windows::open(&handle, &pet) {
                                eprintln!("重啟恢復寵物 {} 失敗：{error}", pet.id);
                            }
                        }
                    }
                    Err(error) => {
                        // DB 失敗時不可誤認沒有寵物，更不可將其標成回籠。
                        eprintln!("重啟讀取出籠寵物失敗：{error}");
                    }
                }
            });
            /*
             * JS Menu.new() 建立的 native menu item
             * 仍會產生 native menu event。
             *
             * Linux 不再依賴 JS action callback
             * 才能 dispatch PetCommand。
             */
            app.on_menu_event(|app_handle, event| {
                // Day32：由生活窗原生選單傳入該隻寵物的 UUID。
                if let Some(id) = control_menu_pet_id(event.id().0.as_str()) {
                    let handle = app_handle.clone();
                    let store = app_handle.state::<pet_store::PetStore>().inner().clone();
                    tauri::async_runtime::spawn(async move {
                        let _operation = pet_windows::lifecycle_guard().await;
                        match store.select(&id).await {
                            Ok(saved) => {
                                // 控制窗原有 Leptos 監聽此事件，選取成功才通知。
                                if let Err(error) = handle.emit("pet-profile-changed", &saved) {
                                    eprintln!("Day32 選取通知失敗：{error}");
                                    return;
                                }
                                if let Err(error) = open_control_window(handle) {
                                    eprintln!("Day32 無法開啟控制窗：{error}");
                                }
                            }
                            Err(error) => eprintln!("Day32 選取寵物失敗：{error}"),
                        }
                    });
                    return;
                }

                if event.id().0.as_str() == "open_control" {
                    let _ = open_control_window(app_handle.clone());
                    return;
                }

                if let Some(scoped) = event.id().0.strip_prefix("pet:") {
                    if let Some((id, action)) = scoped.split_once(':') {
                        let command = match action {
                            "sleep" => Some(PetCommand::Sleep),
                            "wake" => Some(PetCommand::Wake),
                            "interact" => Some(PetCommand::Interact),
                            _ => None,
                        };
                        if let Some(command) = command {
                            let _ = pet_windows::dispatch_and_emit(app_handle, id, command);
                        }
                    }
                    return;
                }

                let command = match event.id().0.as_str() {
                    "interact" => Some(PetCommand::Interact),
                    "sleep" => Some(PetCommand::Sleep),
                    "wake" => Some(PetCommand::Wake),
                    _ => None,
                };

                let Some(command) = command else {
                    return;
                };

                let pet = app_handle.state::<Mutex<PetMachine>>();

                if let Err(error) = dispatch_pet_command(app_handle, pet.inner(), command) {
                    eprintln!("pet context menu command failed: {error}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_pet_state,
            send_pet_command,
            chat_with_pet,
            scan_file_preview,
            scan_duplicate_preview,
            open_control_window,
            quit_pet,
            validate_new_pet_name,
            create_pet,
            list_pets,
            pet_cage_status_by_id,
            active_pet,
            select_pet,
            pet_windows::saved_pet_profile,
            pet_windows::open_saved_pet,
            pet_windows::get_saved_pet_state,
            pet_windows::send_saved_pet_command,
            pet_windows::cage_saved_pet,
            pet_windows::find_saved_pet,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Rust AI Desktop Pet");
}

#[cfg(test)]
mod roam_setting_tests {
    use super::roam_enabled_value;
    #[test]
    fn roam_requires_explicit_opt_in() {
        assert!(!roam_enabled_value(None));
        assert!(!roam_enabled_value(Some("")));
        assert!(!roam_enabled_value(Some("false")));
        assert!(!roam_enabled_value(Some("TRUE")));
        assert!(roam_enabled_value(Some("true")));
        assert!(roam_enabled_value(Some("1")));
    }
}

#[cfg(test)]
mod day21_pet_creation_tests {
    use super::normalize_new_pet_name;
    #[test]
    fn day21_accepts_trimmed_chinese_name() {
        assert_eq!(normalize_new_pet_name("  小橘  "), Ok("小橘".into()));
    }
    #[test]
    fn day21_rejects_blank_and_long_names() {
        assert!(normalize_new_pet_name("  ").is_err());
        assert!(normalize_new_pet_name(&"寵".repeat(33)).is_err());
    }
}

#[cfg(test)]
mod day25_capability_tests {
    #[test]
    fn dynamic_pet_windows_can_listen_without_global_window_wildcard() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let windows = capability["windows"].as_array().unwrap();
        assert!(windows.iter().any(|v| v.as_str() == Some("main")));
        assert!(windows.iter().any(|v| v.as_str() == Some("pet-*")));
        assert!(!windows.iter().any(|v| v.as_str() == Some("*")));
        let permissions = capability["permissions"].as_array().unwrap();
        assert!(
            permissions
                .iter()
                .any(|v| v.as_str() == Some("core:event:default"))
        );
    }
}

#[cfg(test)]
mod day32_control_selection_tests {
    use super::control_menu_pet_id;

    #[test]
    fn open_control_menu_is_scoped_to_one_valid_pet() {
        let id = "123e4567-e89b-42d3-a456-426614174000";
        assert_eq!(
            control_menu_pet_id(&format!("pet:{id}:open_control")),
            Some(id.to_string())
        );
        for invalid in [
            "open_control",
            "pet:other:open_control",
            "pet:123e4567-e89b-42d3-a456-426614174000:sleep",
            "pet:123e4567-e89b-42d3-a456-426614174000:open_control:extra",
        ] {
            assert_eq!(control_menu_pet_id(invalid), None);
        }
    }
}
