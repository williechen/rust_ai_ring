use chat_domain::ChatMessage;

pub(crate) async fn room_history(id: &str) -> Result<Vec<ChatMessage>, String> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("房間識別碼無效".into());
    }
    let text = crate::common::request::text(&format!("/api/rooms/{id}/messages"))
        .await
        .map_err(|error| format!("歷史{error}"))?;
    serde_json::from_str(&text).map_err(|_| "歷史格式無效".into())
}
