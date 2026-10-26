use crate::pet_store::{PetStore, SavedPet};
use pet_application::lifecycle::persist_then_open;
use pet_domain::{PetCommand, PetMachine, PetSnapshot};
use std::{collections::HashMap, sync::Mutex};
use tauri::{Emitter, Manager, State};

// 產品目前單程序；全域序列化所有出／入籠、選取與啟動恢復。
// 不持有 std::sync::MutexGuard 跨 await；Tauri command future 仍可 Send。
static LIFECYCLE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(crate) async fn lifecycle_guard() -> tokio::sync::MutexGuard<'static, ()> {
    LIFECYCLE.lock().await
}

#[derive(Default)]
pub struct Runtime {
    machines: Mutex<HashMap<String, PetMachine>>,
}
impl Runtime {
    pub fn snapshot(&self, id: &str) -> Result<PetSnapshot, String> {
        self.machines
            .lock()
            .map_err(|_| "寵物狀態無法使用")?
            .get(id)
            .map(PetMachine::snapshot)
            .ok_or_else(|| "寵物尚未出籠".into())
    }
    pub fn dispatch(&self, id: &str, command: PetCommand) -> Result<PetSnapshot, String> {
        let mut pets = self.machines.lock().map_err(|_| "寵物狀態無法使用")?;
        let pet = pets.get_mut(id).ok_or("寵物尚未出籠")?;
        pet.dispatch(command).map_err(|e| e.to_string())?;
        Ok(pet.snapshot())
    }
    pub fn remove(&self, id: &str) {
        if let Ok(mut pets) = self.machines.lock() {
            pets.remove(id);
        }
    }
}

pub(crate) fn label(id: &str) -> Result<String, String> {
    let id = uuid::Uuid::parse_str(id).map_err(|_| "寵物ID格式錯誤")?;
    Ok(format!("pet-{}", id.simple()))
}

pub fn open(app: &tauri::AppHandle, pet: &SavedPet) -> Result<(), String> {
    let name = label(&pet.id)?;
    let runtime = app.state::<Runtime>();
    // 同 ID 開窗序列化；重複出籠只 show/focus，不重新建立或重設狀態。
    let mut machines = runtime.machines.lock().map_err(|_| "寵物狀態無法使用")?;
    if let Some(window) = app.get_webview_window(&name) {
        window.show().map_err(|e| e.to_string())?;
        return window.set_focus().map_err(|e| e.to_string());
    }
    let count = machines.len();
    machines.insert(pet.id.clone(), PetMachine::default());
    let result = tauri::WebviewWindowBuilder::new(
        app,
        &name,
        tauri::WebviewUrl::App(format!("index.html?view=pet&pet_id={}", pet.id).into()),
    )
    .title(&pet.name)
    .inner_size(200.0, 200.0)
    .position(
        80.0 + (count % 4) as f64 * 220.0,
        80.0 + (count / 4) as f64 * 220.0,
    )
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .shadow(false)
    .build();
    if let Err(e) = result {
        machines.remove(&pet.id);
        return Err(e.to_string());
    }
    // 僅真正建立新生活窗後啟動；既有窗 show/focus 早退不重複 spawn。
    crate::roam::spawn_for_pet(app.clone(), name);
    Ok(())
}

pub fn dispatch_and_emit(
    app: &tauri::AppHandle,
    id: &str,
    command: PetCommand,
) -> Result<PetSnapshot, String> {
    let snapshot = app.state::<Runtime>().dispatch(id, command)?;
    app.emit(
        "saved-pet-state",
        serde_json::json!({"id":id,"snapshot":snapshot}),
    )
    .map_err(|e| e.to_string())?;
    Ok(snapshot)
}

#[tauri::command]
pub async fn saved_pet_profile(id: String, store: State<'_, PetStore>) -> Result<SavedPet, String> {
    store.profile(&id).await
}

#[tauri::command]
pub fn get_saved_pet_state(id: String, runtime: State<'_, Runtime>) -> Result<PetSnapshot, String> {
    runtime.snapshot(&id)
}

#[tauri::command]
pub fn send_saved_pet_command(
    id: String,
    command: PetCommand,
    app: tauri::AppHandle,
) -> Result<PetSnapshot, String> {
    dispatch_and_emit(&app, &id, command)
}

#[tauri::command]
pub async fn open_saved_pet(
    id: String,
    app: tauri::AppHandle,
    store: State<'_, PetStore>,
) -> Result<(), String> {
    let _operation = lifecycle_guard().await;
    let pet = store.profile(&id).await?;
    let old_caged = store.read_caged(&id).await?;
    persist_then_open(
        old_caged,
        |flag| store.set_caged(&id, flag),
        || open(&app, &pet),
    )
    .await
}

#[tauri::command]
pub async fn cage_saved_pet(
    id: String,
    app: tauri::AppHandle,
    store: State<'_, PetStore>,
) -> Result<(), String> {
    let _operation = lifecycle_guard().await;
    store.profile(&id).await?;
    let window = app.get_webview_window(&label(&id)?);

    // 儲存失敗不能先關窗，也不能清除 Runtime。
    store.set_caged(&id, true).await?;
    if let Some(window) = window {
        if let Err(close_error) = window.close() {
            // 關窗未成功，盡可能恢復 DB 的出籠標記。
            let original = close_error.to_string();
            return match store.set_caged(&id, false).await {
                Ok(()) => Err(format!("關閉寵物視窗失敗，已恢復出籠狀態：{original}")),
                Err(rollback) => Err(format!(
                    "關閉寵物視窗失敗：{original}；回復資料庫亦失敗：{rollback}"
                )),
            };
        }
    }
    app.state::<Runtime>().remove(&id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn day24_more_than_two_pet_states_are_independent_and_caging_is_local() {
        let runtime = Runtime::default();
        for id in ["one", "two", "three"] {
            runtime
                .machines
                .lock()
                .unwrap()
                .insert(id.into(), PetMachine::default());
        }
        let before_two = runtime.snapshot("two").unwrap();
        let before_three = runtime.snapshot("three").unwrap();
        runtime.dispatch("one", PetCommand::Sleep).unwrap();
        assert_eq!(runtime.snapshot("two").unwrap(), before_two);
        assert_eq!(runtime.snapshot("three").unwrap(), before_three);
        runtime.remove("one");
        assert!(runtime.snapshot("one").is_err());
        assert!(runtime.snapshot("two").is_ok());
        assert!(runtime.snapshot("three").is_ok());
    }
    #[test]
    fn day24_window_labels_are_derived_from_each_uuid_not_a_two_window_limit() {
        let labels: std::collections::HashSet<_> = (0..12)
            .map(|_| label(&uuid::Uuid::new_v4().to_string()).unwrap())
            .collect();
        assert_eq!(labels.len(), 12);
        assert!(label("../invalid").is_err());
    }
}

#[cfg(test)]
mod lifecycle_lock_tests {
    use super::*;

    #[tokio::test]
    async fn lifecycle_lock_excludes_a_second_operation() {
        let first = lifecycle_guard().await;
        assert!(LIFECYCLE.try_lock().is_err());
        drop(first);
        assert!(LIFECYCLE.try_lock().is_ok());
    }
}
