use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace=window,js_name=petIdleStatus)]
    fn status_js(id: &str) -> js_sys::Promise;
    #[wasm_bindgen(js_namespace=window,js_name=clearPetIdleChoice)]
    fn clear_js(id: &str) -> js_sys::Promise;
}

pub async fn read(id: &str) -> Result<pet_application::idle_dto::IdleStatus, String> {
    let value = wasm_bindgen_futures::JsFuture::from(status_js(id))
        .await
        .map_err(|_| "無法讀取自然活動")?;
    serde_wasm_bindgen::from_value(value).map_err(|_| "自然活動資料格式錯誤".into())
}

pub async fn clear(id: &str) -> Result<(), String> {
    wasm_bindgen_futures::JsFuture::from(clear_js(id))
        .await
        .map_err(|_| "無法儲存閒置資格")?;
    Ok(())
}
