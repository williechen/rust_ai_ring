use shared::{CreateRoomRequest, CreateRoomResponse};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode};

pub async fn create_room(name: &str) -> Result<CreateRoomResponse, String> {
    let body = serde_json::to_string(&CreateRoomRequest {
        name: name.to_string(),
    })
    .map_err(|error| format!("failed to encode create room request: {error}"))?;

    let init = RequestInit::new();
    init.set_method("POST");
    init.set_mode(RequestMode::SameOrigin);
    init.set_body(&JsValue::from_str(&body));

    let request = Request::new_with_str_and_init("/api/rooms", &init)
        .map_err(|error| format!("failed to build create room request: {error:?}"))?;

    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|error| format!("failed to set content type: {error:?}"))?;

    let window = web_sys::window().ok_or_else(|| "browser window is unavailable".to_string())?;

    let response_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|error| format!("create room request failed: {error:?}"))?;

    let response: web_sys::Response = response_value
        .dyn_into()
        .map_err(|_| "create room response is not an HTTP Response".to_string())?;

    if !response.ok() {
        return Err(format!(
            "create room failed with HTTP status: {}",
            response.status()
        ));
    }

    let response_text = JsFuture::from(
        response
            .text()
            .map_err(|error| format!("failed to read response body: {error:?}"))?,
    )
    .await
    .map_err(|error| format!("failed to await response body: {error:?}"))?
    .as_string()
    .ok_or_else(|| "create room response body is not text".to_string())?;

    serde_json::from_str::<CreateRoomResponse>(&response_text)
        .map_err(|error| format!("failed to decode create room response: {error}"))
}

// 接續既有 create_room；不重建另一個 HTTP client。
pub async fn list_rooms() -> Result<Vec<(String, String)>, String> {
    search_rooms("").await
}

pub async fn search_rooms(query: &str) -> Result<Vec<(String, String)>, String> {
    let params = web_sys::UrlSearchParams::new().map_err(|_| "無法建立查詢")?;
    params.append("q", query);
    let url = format!("/api/rooms?{}", params.to_string());
    let window = web_sys::window().ok_or("無法取得瀏覽器")?;
    let value = JsFuture::from(window.fetch_with_str(&url))
        .await
        .map_err(|_| "房間讀取失敗")?;
    let response: web_sys::Response = value.dyn_into().map_err(|_| "無效回應")?;
    if !response.ok() {
        return Err(format!("讀取失敗：{}", response.status()));
    }
    let text = JsFuture::from(response.text().map_err(|_| "無法讀取回應")?)
        .await
        .map_err(|_| "無法讀取回應")?
        .as_string()
        .ok_or("無效文字")?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&text).map_err(|_| "房間格式錯誤")?;
    rows.into_iter()
        .map(|row| {
            let id = row["id"].as_str().ok_or("缺少房間 ID")?;
            let name = row["name"].as_str().ok_or("缺少房間名稱")?;
            // 此學習入口沿用現有 UUID／fixture ID，不把任意字串當 URL。
            if id.is_empty()
                || !id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            {
                return Err("房間 ID 不適用目前入口".into());
            }
            Ok((id.to_owned(), name.to_owned()))
        })
        .collect()
}

pub async fn session_status() -> Result<bool, String> {
    let window = web_sys::window().ok_or("無法取得瀏覽器")?;
    let value = JsFuture::from(window.fetch_with_str("/api/session"))
        .await
        .map_err(|_| "無法確認登入狀態")?;
    let response: web_sys::Response = value.dyn_into().map_err(|_| "無效 Session 回應")?;
    match response.status() {
        204 => Ok(true),
        401 => Ok(false),
        code => Err(format!("Session 查詢失敗：HTTP {code}")),
    }
}
