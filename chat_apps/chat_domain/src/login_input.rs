/// 依既有協定以UTF-8位元組檢查帳密輸入界線，不判斷身分是否存在。
pub fn valid_credentials(name: &str, password: &str) -> bool {
    !name.is_empty() && name.len() <= 32 && !password.is_empty() && password.len() <= 128
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_limits_preserve_unicode_and_password_spaces() {
        assert!(valid_credentials(&"帳".repeat(10), " "));
        assert!(!valid_credentials(&"帳".repeat(11), "password"));
        assert!(!valid_credentials("", "password"));
        assert!(!valid_credentials("member", &"x".repeat(129)));
    }
}
