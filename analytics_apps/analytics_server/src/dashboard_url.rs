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
}
