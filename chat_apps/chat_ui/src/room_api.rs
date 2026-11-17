use crate::room_list_state::RoomListItem;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

pub async fn room_history(id: &str) -> Result<Vec<chat_domain::ChatMessage>, String> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("房間識別碼無效".into());
    }
    let window = web_sys::window().ok_or("無法取得瀏覽器")?;
    let value = JsFuture::from(window.fetch_with_str(&format!("/api/rooms/{id}/messages")))
        .await
        .map_err(|_| "歷史讀取失敗")?;
    let response: web_sys::Response = value.dyn_into().map_err(|_| "歷史回應無效")?;
    if !response.ok() {
        return Err(format!("歷史讀取失敗：HTTP {}", response.status()));
    }
    let text = JsFuture::from(response.text().map_err(|_| "歷史內容無效")?)
        .await
        .map_err(|_| "歷史內容無效")?
        .as_string()
        .ok_or("歷史內容無效")?;
    serde_json::from_str(&text).map_err(|_| "歷史格式無效".into())
}

/// 公開房間清單與搜尋共用既有的查詢端點。
pub async fn list_rooms() -> Result<Vec<RoomListItem>, String> {
    search_rooms("").await
}

pub async fn search_rooms(query: &str) -> Result<Vec<RoomListItem>, String> {
    search_rooms_in_category(query, "").await
}

pub async fn search_rooms_in_category(
    query: &str,
    category: &str,
) -> Result<Vec<RoomListItem>, String> {
    let params = web_sys::UrlSearchParams::new().map_err(|_| "無法建立查詢")?;
    params.append("q", query);
    params.append("labels", "true");
    params.append("category", category);
    let url = format!("/api/rooms?{}", params.to_string());
    let text = crate::directory_request::text(&url).await?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&text).map_err(|_| "房間格式錯誤")?;
    rows.into_iter()
        .map(|row| {
            let id = row["id"].as_str().ok_or("缺少房間 ID")?;
            let name = row["name"].as_str().ok_or("缺少房間名稱")?;

            if id.is_empty()
                || !id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            {
                return Err("房間 ID 不適用目前入口".into());
            }
            Ok(RoomListItem {
                id: id.into(),
                name: name.into(),
                category_label: row["category_label"].as_str().map(str::to_owned),
            })
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

/// Uses the existing public room-detail endpoint; no new server or database API.
pub async fn room_name(id: &str) -> Result<String, String> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("無效房間 ID".into());
    }
    let window = web_sys::window().ok_or("無法取得瀏覽器")?;
    let value = JsFuture::from(window.fetch_with_str(&format!("/api/rooms/{id}")))
        .await
        .map_err(|_| "房間資料讀取失敗")?;
    let response: web_sys::Response = value.dyn_into().map_err(|_| "無效房間回應")?;
    if !response.ok() {
        return Err(format!("房間資料 HTTP {}", response.status()));
    }
    let text = JsFuture::from(response.text().map_err(|_| "無法讀取房間資料")?)
        .await
        .map_err(|_| "無法讀取房間資料")?
        .as_string()
        .ok_or("房間資料不是文字")?;
    let row: serde_json::Value = serde_json::from_str(&text).map_err(|_| "房間資料格式錯誤")?;
    if row["id"].as_str() != Some(id) {
        return Err("房間資料 ID 不一致".into());
    }
    row["name"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "房間名稱缺失".into())
}

/// 只由已登入會員的受保護能力端點讀取登出 CSRF，不從 Cookie 推算。
pub async fn logout_csrf() -> Result<String, String> {
    let window = web_sys::window().ok_or("無法取得瀏覽器")?;
    let value = JsFuture::from(window.fetch_with_str("/api/member-capabilities"))
        .await
        .map_err(|_| "無法取得登出權杖")?;
    let response: web_sys::Response = value.dyn_into().map_err(|_| "登出權杖回應無效")?;
    if !response.ok() {
        return Err("無法確認登出權限".into());
    }
    let text = JsFuture::from(response.text().map_err(|_| "無法讀取權杖")?)
        .await
        .map_err(|_| "無法讀取權杖")?
        .as_string()
        .ok_or("權杖格式無效")?;
    let data: serde_json::Value = serde_json::from_str(&text).map_err(|_| "權杖格式無效")?;
    data.get("logout_csrf")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "缺少登出權杖".into())
}

pub async fn room_categories() -> Result<Vec<crate::room_list_state::CategoryItem>, String> {
    let text = crate::directory_request::text("/api/room-categories").await?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&text).map_err(|_| "分類格式無效")?;
    rows.into_iter()
        .map(|row| {
            Ok(crate::room_list_state::CategoryItem {
                code: row["code"].as_str().ok_or("分類代碼缺失")?.into(),
                label: row["label"].as_str().ok_or("分類名稱缺失")?.into(),
            })
        })
        .collect()
}

pub async fn popular_rooms() -> Result<Vec<crate::room_list_state::PopularRoomItem>, String> {
    let window = web_sys::window().ok_or("無法取得瀏覽器")?;
    let options = web_sys::RequestInit::new();
    options.set_cache(web_sys::RequestCache::NoStore);
    let value = JsFuture::from(window.fetch_with_str_and_init("/api/rooms?popular=true", &options))
        .await
        .map_err(|_| "熱門聊天室讀取失敗")?;
    let response: web_sys::Response = value.dyn_into().map_err(|_| "熱門聊天室回應無效")?;
    if !response.ok() {
        return Err(format!("熱門聊天室讀取失敗：HTTP {}", response.status()));
    }
    let text = JsFuture::from(response.text().map_err(|_| "熱門聊天室內容無效")?)
        .await
        .map_err(|_| "熱門聊天室內容無效")?
        .as_string()
        .ok_or("熱門聊天室內容無效")?;
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&text).map_err(|_| "熱門聊天室格式無效")?;
    if rows.len() > 10 {
        return Err("熱門聊天室數量無效".into());
    }
    rows.into_iter()
        .map(|row| {
            let id = row["id"]
                .as_str()
                .filter(|id| {
                    !id.is_empty()
                        && id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                })
                .ok_or("房間識別無效")?;
            Ok(crate::room_list_state::PopularRoomItem {
                id: id.into(),
                name: row["name"].as_str().ok_or("房間名稱無效")?.into(),
                online_count: usize::try_from(row["online_count"].as_u64().ok_or("在線人數無效")?)
                    .map_err(|_| "在線人數無效")?,
            })
        })
        .collect()
}

pub async fn room_members(id: &str) -> Result<Vec<String>, String> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("房間識別碼無效".into());
    }
    let options = web_sys::RequestInit::new();
    options.set_cache(web_sys::RequestCache::NoStore);
    let value = JsFuture::from(
        web_sys::window()
            .ok_or("無法取得瀏覽器")?
            .fetch_with_str_and_init(&format!("/api/rooms/{id}/members"), &options),
    )
    .await
    .map_err(|_| "人員名單暫時無法更新")?;
    let response: web_sys::Response = value.dyn_into().map_err(|_| "人員回應無效")?;
    if !response.ok() {
        return Err(format!("人員名單讀取失敗：HTTP {}", response.status()));
    }
    let text = JsFuture::from(response.text().map_err(|_| "人員內容無效")?)
        .await
        .map_err(|_| "人員內容無效")?
        .as_string()
        .ok_or("人員內容無效")?;
    serde_json::from_str(&text).map_err(|_| "人員格式無效".into())
}
