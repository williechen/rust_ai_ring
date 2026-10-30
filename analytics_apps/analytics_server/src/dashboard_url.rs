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

/// Reuse the exact-kind encoder for the authenticated CSV export link.
pub(crate) fn export_csv_url(source: &str, event_type: &str) -> String {
    if event_type.is_empty() {
        format!("/api/v1/events/{source}/export.csv")
    } else {
        format!(
            "/api/v1/events/{source}/export.csv?event_type={}",
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

#[cfg(test)]
mod export_url_refactor_tests {
    use super::*;
    #[test]
    fn csv_url_keeps_exact_kind_and_no_filter_contract() {
        assert_eq!(
            export_csv_url("web_chat", ""),
            "/api/v1/events/web_chat/export.csv"
        );
        assert_eq!(
            export_csv_url("desktop_pet", "pet.中文 &"),
            "/api/v1/events/desktop_pet/export.csv?event_type=pet.%E4%B8%AD%E6%96%87%20%26"
        );
    }
}
