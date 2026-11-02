/// 來源識別只允許既有兩種精確值，不以未知字串推測授權。
pub(crate) fn source_label(source: &str) -> Option<&'static str> {
    match source {
        "web_chat" => Some("Chat"),
        "desktop_pet" => Some("Pet"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::source_label;
    #[test]
    fn exact_source_labels_are_preserved() {
        assert_eq!(source_label("web_chat"), Some("Chat"));
        assert_eq!(source_label("desktop_pet"), Some("Pet"));
        assert_eq!(source_label("unknown"), None);
        assert_eq!(source_label("WEB_CHAT"), None);
    }
}
