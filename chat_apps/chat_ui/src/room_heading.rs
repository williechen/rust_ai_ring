// 房名讀取成功與失敗時使用同一顯示契約，供 SSR 與瀏覽器共用。
pub(super) fn room_heading(id: &str, result: Result<String, String>) -> String {
    match result {
        Ok(name) => format!("房間 {name}"),
        Err(_) => format!("房間 {id}（無法取得房間名稱）"),
    }
}

#[cfg(test)]
mod tests {
    use super::room_heading;

    #[test]
    fn success_and_failure_preserve_existing_visible_contract() {
        assert_eq!(
            room_heading("room-1", Ok("技術討論".into())),
            "房間 技術討論"
        );
        assert_eq!(
            room_heading("room-1", Err("404".into())),
            "房間 room-1（無法取得房間名稱）"
        );
    }
}
