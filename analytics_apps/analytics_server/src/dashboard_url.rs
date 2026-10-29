pub(crate) fn encode_query_component(value: &str) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            result.push(char::from(byte));
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

/// Keep the exact event-type filter when switching the authenticated dashboard source.
pub(crate) fn source_switch_url(source: &str, event_type: &str) -> String {
    if event_type.is_empty() {
        format!("/dashboard?source={source}")
    } else {
        format!(
            "/dashboard?source={source}&event_type={}",
            encode_query_component(event_type)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::encode_query_component;

    #[test]
    fn existing_csv_url_contract_keeps_utf8_and_reserved_characters() {
        assert_eq!(
            encode_query_component("a &中文"),
            "a%20%26%E4%B8%AD%E6%96%87"
        );
        assert_eq!(encode_query_component("a-b_c.~"), "a-b_c.~");
        assert_eq!(encode_query_component("<script>"), "%3Cscript%3E");
    }

    #[test]
    fn source_switch_preserves_encoded_filter_and_empty_semantics() {
        assert_eq!(
            super::source_switch_url("desktop_pet", "a &中文"),
            "/dashboard?source=desktop_pet&event_type=a%20%26%E4%B8%AD%E6%96%87"
        );
        assert_eq!(
            super::source_switch_url("web_chat", ""),
            "/dashboard?source=web_chat"
        );
    }
}
