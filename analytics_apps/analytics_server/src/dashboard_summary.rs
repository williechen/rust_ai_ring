pub(crate) fn summary_count_label(source_label: &str, event_type: &str, total: i64) -> String {
    if event_type.is_empty() {
        format!("{source_label} 全部事件類型：{total} 筆已提交事件")
    } else {
        format!("{source_label} 事件類型「{event_type}」：{total} 筆已提交事件")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn committed_count_caption_preserves_source_kind_and_zero() {
        assert_eq!(
            summary_count_label("Chat", "", 2),
            "Chat 全部事件類型：2 筆已提交事件"
        );
        assert_eq!(
            summary_count_label("Pet", "pet.睡覺", 0),
            "Pet 事件類型「pet.睡覺」：0 筆已提交事件"
        );
    }
}
