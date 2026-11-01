// 訊息內容不可僅有空白；保留原始內容，不自動裁切。
pub fn valid_message_content(content: &str) -> bool {
    !content.trim().is_empty()
}

// 防止跨房間發送；空白識別碼不可視為合法房間。
pub fn room_matches_connection(connection: &str, event: &str) -> bool {
    !connection.trim().is_empty() && !event.trim().is_empty() && connection == event
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matching_room() {
        assert!(room_matches_connection("room-a", "room-a"));
    }
    #[test]
    fn different_room() {
        assert!(!room_matches_connection("room-a", "room-b"));
    }
    #[test]
    fn blank_room() {
        assert!(!room_matches_connection("", ""));
        assert!(!room_matches_connection("room-a", " "));
    }
    #[test]
    fn accepts_nonblank_content() {
        assert!(valid_message_content("你好"));
        assert!(valid_message_content(" 你好 "));
    }
    #[test]
    fn rejects_blank_content() {
        assert!(!valid_message_content(""));
        assert!(!valid_message_content(" \n\t "));
    }
}
