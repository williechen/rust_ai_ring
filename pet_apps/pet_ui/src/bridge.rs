use js_sys::{Function, Promise};
use pet_domain::{PetCommand, PetSnapshot};
use wasm_bindgen::{JsCast, closure::Closure, prelude::*};
use wasm_bindgen_futures::JsFuture;

use crate::saved_pet::SavedPet;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace=window,js_name=loadPetSettings)]
    fn load_pet_settings_js(id: &str) -> Promise;

    #[wasm_bindgen(js_namespace=window,js_name=savePetSettings)]
    fn save_pet_settings_js(id: &str, profile: JsValue) -> Promise;

    #[wasm_bindgen(
        js_namespace=window,
        js_name = isPetWindow
    )]
    pub fn is_pet_window() -> bool;

    #[wasm_bindgen(
        js_namespace=window,
        js_name = openPetControls
    )]
    pub fn open_controls_js() -> Promise;

    #[wasm_bindgen(
        js_namespace=window,
        js_name = quitPet
    )]
    pub fn quit_pet_js() -> Promise;

    #[wasm_bindgen(
        js_namespace = window,
        js_name = getPetState
    )]
    fn get_pet_state_js(id: Option<String>) -> Promise;

    #[wasm_bindgen(
        js_namespace = window,
        js_name = sendPetCommand
    )]
    fn send_pet_command_js(command: JsValue, id: Option<String>) -> Promise;

    #[wasm_bindgen(
        js_namespace = window,
        js_name = listenPetState
    )]
    fn listen_pet_state_js(callback: &Function) -> Promise;

    #[wasm_bindgen(
        js_namespace = window,
        js_name = showPetContextMenu
    )]
    fn show_pet_context_menu_js(x: i32, y: i32) -> Promise;

    #[wasm_bindgen(
        js_namespace = window,
        js_name = chatWithPet
    )]
    fn chat_with_pet_js(message: &str) -> Promise;

    #[wasm_bindgen(js_namespace = window, js_name = listPets)]
    fn list_pets_js() -> Promise;

    #[wasm_bindgen(js_namespace = window, js_name = createPet)]
    fn create_pet_js(name: &str) -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=renamePet)]
    fn rename_pet_js(id: &str, name: &str) -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=openSavedPet)]
    fn open_saved_pet_js(id: &str) -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=cageSavedPet)]
    fn cage_saved_pet_js() -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=cageSavedPetById)]
    fn cage_saved_pet_by_id_js(id: &str) -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=petCageStatusById)]
    fn pet_cage_status_by_id_js(id: &str) -> Promise;

    #[wasm_bindgen(js_namespace=window,js_name=petActivityStatus)]
    fn pet_activity_status_js(id: &str) -> Promise;

    #[wasm_bindgen(js_namespace=window,js_name=setPetWalk)]
    fn set_pet_walk_js(id: &str, walking: bool) -> Promise;

    #[wasm_bindgen(js_namespace=window,js_name=setPetDefaultCaged)]
    fn set_pet_default_caged_js(id: &str, caged: bool) -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=findSavedPet)]
    fn find_saved_pet_js(id: &str) -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=activePet)]
    fn active_pet_js() -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=selectPet)]
    fn select_pet_js(id: &str) -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=listenPetProfile)]
    fn listen_pet_profile_js(callback: &Function) -> Promise;

    #[wasm_bindgen(js_namespace=window,js_name=startupRestoreFailures)]
    fn startup_restore_failures_js() -> Promise;

    #[wasm_bindgen(js_namespace=window,js_name=listenStartupRestoreFailures)]
    fn listen_startup_restore_failures_js(callback: &Function) -> Promise;

}

// 將 JavaScript 的錯誤轉換為 Rust 的字串
fn js_error(value: JsValue) -> String {
    value.as_string().unwrap_or_else(|| format!("{value:?}"))
}

// 獲取寵物的當前狀態
pub async fn get_pet_state() -> Result<PetSnapshot, String> {
    get_pet_state_for(None).await
}

pub async fn get_pet_state_for(id: Option<String>) -> Result<PetSnapshot, String> {
    let value = JsFuture::from(get_pet_state_js(id))
        .await
        .map_err(js_error)?;

    serde_wasm_bindgen::from_value(value).map_err(|error| error.to_string())
}

// 發送指令給寵物並獲取更新後的狀態
pub async fn send_pet_command_for(
    command: PetCommand,
    id: Option<String>,
) -> Result<PetSnapshot, String> {
    let command = serde_wasm_bindgen::to_value(&command).map_err(|error| error.to_string())?;

    let value = JsFuture::from(send_pet_command_js(command, id))
        .await
        .map_err(js_error)?;

    serde_wasm_bindgen::from_value(value).map_err(|error| error.to_string())
}

// 監聽寵物狀態的變化
pub async fn listen_pet_state(
    mut on_snapshot: impl FnMut(PetSnapshot) + 'static,
) -> Result<(), String> {
    let callback = Closure::<dyn FnMut(JsValue)>::new(move |value| {
        if let Ok(snapshot) = serde_wasm_bindgen::from_value(value) {
            on_snapshot(snapshot);
        }
    });

    JsFuture::from(listen_pet_state_js(callback.as_ref().unchecked_ref()))
        .await
        .map_err(js_error)?;

    // listener 跟著整個 pet app 存活。
    callback.forget();

    Ok(())
}

// 顯示寵物的右鍵選單
pub async fn show_pet_context_menu(x: i32, y: i32) -> Result<(), String> {
    JsFuture::from(show_pet_context_menu_js(x, y))
        .await
        .map_err(js_error)?;

    Ok(())
}

// 與寵物進行聊天
pub async fn chat_with_pet(message: String) -> Result<String, String> {
    let value = JsFuture::from(chat_with_pet_js(&message))
        .await
        .map_err(js_error)?;

    value
        .as_string()
        .ok_or_else(|| "Failed to convert chat_with_pet result to string".to_string())
}

#[derive(Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntry {
    pub name: String,
    pub is_directory: bool,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<u64>,
}

#[derive(serde::Deserialize)]
pub struct DirectoryListing {
    pub entries: Vec<DirectoryEntry>,
    pub skipped: usize,
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = browseAuthorizedDirectory)]
    fn browse_authorized_directory_js(path: JsValue) -> Promise;
}

pub async fn browse_authorized_directory(path: &[String]) -> Result<DirectoryListing, String> {
    let path = serde_wasm_bindgen::to_value(path).map_err(|_| "目錄路徑格式不正確")?;
    let result = JsFuture::from(browse_authorized_directory_js(path))
        .await
        .map_err(js_error)?;
    serde_wasm_bindgen::from_value(result).map_err(|_| "目錄清單格式不正確".into())
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonRow {
    pub original: Vec<String>,
    pub proposed: Vec<String>,
    pub size_bytes: u64,
}

#[derive(Clone, serde::Deserialize)]
pub struct DirectoryComparison {
    pub original: Vec<ComparisonRow>,
    pub organized: Vec<ComparisonRow>,
    pub duplicates: Vec<ComparisonRow>,
    pub skipped: usize,
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = compareAuthorizedDirectory)]
    fn compare_authorized_directory_js(path: JsValue) -> Promise;
}

pub async fn compare_authorized_directory(path: &[String]) -> Result<DirectoryComparison, String> {
    let path = serde_wasm_bindgen::to_value(path).map_err(|_| "目錄路徑格式不正確")?;
    let value = JsFuture::from(compare_authorized_directory_js(path))
        .await
        .map_err(js_error)?;
    serde_wasm_bindgen::from_value(value).map_err(|_| "目錄比對格式不正確".into())
}

pub async fn open_controls() -> Result<(), String> {
    JsFuture::from(open_controls_js()).await.map_err(js_error)?;

    Ok(())
}

pub async fn quit_pet() -> Result<(), String> {
    JsFuture::from(quit_pet_js()).await.map_err(js_error)?;

    Ok(())
}

pub async fn create_pet(name: String) -> Result<SavedPet, String> {
    let value = JsFuture::from(create_pet_js(&name))
        .await
        .map_err(js_error)?;
    serde_wasm_bindgen::from_value(value)
        .map_err(|_| "建立寵物回應格式錯誤；請重新整理清單確認是否已建立".into())
}

pub async fn rename_pet(id: &str, name: &str) -> Result<SavedPet, String> {
    let value = JsFuture::from(rename_pet_js(id, name))
        .await
        .map_err(js_error)?;
    serde_wasm_bindgen::from_value(value).map_err(|_| "寵物更名回應格式錯誤，請重新載入清單".into())
}

pub async fn list_pets() -> Result<Vec<SavedPet>, String> {
    let value = JsFuture::from(list_pets_js()).await.map_err(js_error)?;
    serde_wasm_bindgen::from_value(value).map_err(|_| "寵物清單格式錯誤".into())
}

pub async fn active_pet() -> Result<Option<SavedPet>, String> {
    let value = JsFuture::from(active_pet_js()).await.map_err(js_error)?;
    serde_wasm_bindgen::from_value(value).map_err(|_| "生活窗資料格式錯誤".into())
}

pub async fn select_pet(id: &str) -> Result<SavedPet, String> {
    let value = JsFuture::from(select_pet_js(id)).await.map_err(js_error)?;
    serde_wasm_bindgen::from_value(value).map_err(|_| "選取資料格式錯誤".into())
}

pub async fn listen_pet_profile(
    mut callback: impl FnMut(SavedPet) + 'static,
) -> Result<(), String> {
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |v| {
        if let Ok(pet) = serde_wasm_bindgen::from_value(v) {
            callback(pet);
        }
    });
    JsFuture::from(listen_pet_profile_js(closure.as_ref().unchecked_ref()))
        .await
        .map_err(js_error)?;
    closure.forget(); // 與此單一 App 視窗同壽命。
    Ok(())
}

pub async fn startup_restore_failures()
-> Result<Vec<pet_application::lifecycle::StartupRestoreFailure>, String> {
    let rows = JsFuture::from(startup_restore_failures_js())
        .await
        .map_err(js_error)?;
    serde_wasm_bindgen::from_value(rows).map_err(|_| "啟動恢復結果格式錯誤".to_owned())
}

/// 訂閱成功後取得快照；事件先到或快照先到皆以 UUID 去重。
pub async fn listen_startup_restore_failures(
    mut callback: impl FnMut(pet_application::lifecycle::StartupRestoreFailure) + 'static,
) -> Result<(), String> {
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |value| {
        if let Ok(failure) = serde_wasm_bindgen::from_value(value) {
            callback(failure);
        }
    });
    JsFuture::from(listen_startup_restore_failures_js(
        closure.as_ref().unchecked_ref(),
    ))
    .await
    .map_err(js_error)?;
    closure.forget();
    Ok(())
}

pub async fn open_saved_pet(id: &str) -> Result<(), String> {
    JsFuture::from(open_saved_pet_js(id))
        .await
        .map_err(js_error)?;
    Ok(())
}

pub async fn cage_saved_pet() -> Result<(), String> {
    JsFuture::from(cage_saved_pet_js())
        .await
        .map_err(js_error)?;
    Ok(())
}

pub async fn cage_saved_pet_by_id(id: &str) -> Result<(), String> {
    JsFuture::from(cage_saved_pet_by_id_js(id))
        .await
        .map_err(js_error)?;
    Ok(())
}

/// Query the existing SQLite cage flag through the registered Tauri IPC command.
pub async fn pet_cage_status_by_id(id: &str) -> Result<bool, String> {
    let value = JsFuture::from(pet_cage_status_by_id_js(id))
        .await
        .map_err(js_error)?;
    value.as_bool().ok_or_else(|| "回籠狀態回應格式錯誤".into())
}

pub async fn find_saved_pet(id: &str) -> Result<(), String> {
    JsFuture::from(find_saved_pet_js(id))
        .await
        .map_err(js_error)?;
    Ok(())
}

#[derive(Clone, serde::Deserialize)]
pub struct ActivityStatus {
    pub outside: bool,
    pub roaming: bool,
    #[serde(default)]
    pub sitting: bool,
    pub default_caged: bool,
    pub walk_label: Option<String>,
    pub sit_label: Option<String>,
}

pub async fn activity_action(id: &str, choice: i8) -> Result<ActivityStatus, String> {
    let promise = match choice {
        1 => set_pet_walk_js(id, true),
        2 => set_pet_walk_js(id, false),
        3 => set_pet_default_caged_js(id, false),
        4 => set_pet_default_caged_js(id, true),
        _ => pet_activity_status_js(id),
    };
    let value = JsFuture::from(promise).await.map_err(js_error)?;
    serde_wasm_bindgen::from_value(value).map_err(|_| "活動資料格式錯誤".into())
}

pub async fn load_pet_settings(id: &str) -> Result<pet_domain::profile::PetProfile, String> {
    let value = JsFuture::from(load_pet_settings_js(id))
        .await
        .map_err(js_error)?;
    serde_wasm_bindgen::from_value(value).map_err(|_| "設定回應格式錯誤".into())
}

pub async fn save_pet_settings(
    id: &str,
    p: &pet_domain::profile::PetProfile,
) -> Result<pet_domain::profile::PetProfile, String> {
    let value = serde_wasm_bindgen::to_value(p).map_err(|_| "設定內容格式錯誤")?;
    let value = JsFuture::from(save_pet_settings_js(id, value))
        .await
        .map_err(js_error)?;
    serde_wasm_bindgen::from_value(value).map_err(|_| "設定回應格式錯誤".into())
}
