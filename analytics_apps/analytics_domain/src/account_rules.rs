/// 帳號識別碼使用既有 ASCII 契約，避免大小寫折疊後產生歧義。
pub fn valid_viewer_username(value: &str) -> bool {
    (3..=64).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewer_username_keeps_original_contract() {
        assert!(valid_viewer_username("reader_01"));
        assert!(valid_viewer_username("ABC-123"));
        assert!(!valid_viewer_username("ab"));
        assert!(!valid_viewer_username("中"));
        assert!(!valid_viewer_username("a b"));
        assert!(!valid_viewer_username(&"x".repeat(65)));
    }
}
