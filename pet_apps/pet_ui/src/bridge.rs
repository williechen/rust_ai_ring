use js_sys::{Function, Promise};
use pet_domain::{PetCommand, PetSnapshot};
use wasm_bindgen::{JsCast, closure::Closure, prelude::*};
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen]
extern "C" {

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
    fn get_pet_state_js() -> Promise;

    #[wasm_bindgen(
        js_namespace = window,
        js_name = sendPetCommand
    )]
    fn send_pet_command_js(command: JsValue) -> Promise;

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

    #[wasm_bindgen(
        js_namespace = window,
        js_name = scanFilePreview
    )]
    fn scan_file_preview_js() -> Promise;

    #[wasm_bindgen(js_namespace = window, js_name = scanDuplicatePreview)]
    fn scan_duplicate_preview_js() -> Promise;

    #[wasm_bindgen(js_namespace = window, js_name = validateNewPetName)]
    fn validate_new_pet_name_js(name: &str) -> Promise;

    #[wasm_bindgen(js_namespace = window, js_name = listPets)]
    fn list_pets_js() -> Promise;

    #[wasm_bindgen(js_namespace = window, js_name = createPet)]
    fn create_pet_js(name: &str) -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=openSavedPet)]
    fn open_saved_pet_js(id: &str) -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=cageSavedPet)]
    fn cage_saved_pet_js() -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=cageSavedPetById)]
    fn cage_saved_pet_by_id_js(id: &str) -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=findSavedPet)]
    fn find_saved_pet_js(id: &str) -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=activePet)]
    fn active_pet_js() -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=selectPet)]
    fn select_pet_js(id: &str) -> Promise;

    #[wasm_bindgen(js_namespace=window, js_name=listenPetProfile)]
    fn listen_pet_profile_js(callback: &Function) -> Promise;
}

// 將 JavaScript 的錯誤轉換為 Rust 的字串
fn js_error(value: JsValue) -> String {
    value.as_string().unwrap_or_else(|| format!("{value:?}"))
}

// 獲取寵物的當前狀態
pub async fn get_pet_state() -> Result<PetSnapshot, String> {
    let value = JsFuture::from(get_pet_state_js()).await.map_err(js_error)?;

    serde_wasm_bindgen::from_value(value).map_err(|error| error.to_string())
}

// 發送指令給寵物並獲取更新後的狀態
pub async fn send_pet_command(command: PetCommand) -> Result<PetSnapshot, String> {
    let command = serde_wasm_bindgen::to_value(&command).map_err(|error| error.to_string())?;

    let value = JsFuture::from(send_pet_command_js(command))
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

pub async fn scan_file_preview() -> Result<String, String> {
    let value = JsFuture::from(scan_file_preview_js())
        .await
        .map_err(js_error)?;
    let view: crate::file_scan_view::FileScanView =
        serde_wasm_bindgen::from_value(value).map_err(|error| error.to_string())?;
    Ok(view.summary())
}

pub async fn scan_duplicate_preview() -> Result<String, String> {
    let value = JsFuture::from(scan_duplicate_preview_js())
        .await
        .map_err(js_error)?;
    let result: crate::duplicate_preview::DuplicatePreviewView =
        serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string())?;
    Ok(result.summary())
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

+#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct SavedPet {
    pub id: String,
    pub name: String,
    pub species: String,
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

pub async fn find_saved_pet(id: &str) -> Result<(), String> {
    JsFuture::from(find_saved_pet_js(id))
        .await
        .map_err(js_error)?;
    Ok(())
}
