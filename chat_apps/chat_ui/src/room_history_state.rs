use chat_domain::ChatMessage;
use std::collections::HashSet;

/// 快照可能比 WebSocket 晚到，依訊息 ID 合併且不覆蓋即時事件。
pub fn merge_history(current: Vec<ChatMessage>, history: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut seen = HashSet::new();
    history
        .into_iter()
        .chain(current)
        .filter(|m| seen.insert(m.id.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chat_domain::MessageAuthor;
    fn message(id: &str) -> ChatMessage {
        ChatMessage {
            id: id.into(),
            room_id: "r".into(),
            author: MessageAuthor::AnonymousUser,
            content: id.into(),
        }
    }
    #[test]
    fn late_history_preserves_live_events_without_duplicates() {
        let rows = merge_history(
            vec![message("live"), message("same")],
            vec![message("old"), message("same")],
        );
        assert_eq!(
            rows.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            vec!["old", "same", "live"]
        );
    }
}
