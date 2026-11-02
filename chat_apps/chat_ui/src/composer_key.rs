/// 中文輸入法組字期間不可送出；Shift+Enter 保留多行輸入。
pub(super) fn should_send_key(
    key: &str,
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
    composing: bool,
    key_code: u32,
) -> bool {
    key == "Enter" && !shift && !ctrl && !alt && !meta && !composing && key_code != 229
}

#[cfg(test)]
mod tests {
    use super::should_send_key;

    #[test]
    fn enter_sends_but_shift_enter_keeps_newline() {
        assert!(should_send_key(
            "Enter", false, false, false, false, false, 13
        ));
        assert!(!should_send_key(
            "Enter", true, false, false, false, false, 13
        ));
        assert!(!should_send_key(
            "Enter", false, true, false, false, false, 13
        ));
        assert!(!should_send_key(
            "Enter", false, false, true, false, false, 13
        ));
        assert!(!should_send_key(
            "Enter", false, false, false, true, false, 13
        ));
    }

    #[test]
    fn ime_and_other_keys_never_submit() {
        assert!(!should_send_key(
            "Enter", false, false, false, false, true, 13
        ));
        assert!(!should_send_key(
            "Enter", false, false, false, false, false, 229
        ));
        assert!(!should_send_key("a", false, false, false, false, false, 65));
    }
}
