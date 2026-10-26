pub mod lifecycle;

/// 建立寵物的輸入規則；不持有 Tauri、SQLx 或 OS driver。
pub fn normalize_new_pet_name(input: &str) -> Result<String, String> {
    // 必須在 trim 前檢查；避免尾端換行被當一般空白悄悄吃掉。
    if input.chars().any(char::is_control) {
        return Err("寵物名稱不得含控制字元".into());
    }
    let name = input.trim();
    if name.is_empty() || name.chars().count() > 32 {
        return Err("寵物名稱須為 1 到 32 字元".into());
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod day34_name_tests {
    use super::normalize_new_pet_name;

    #[test]
    fn normalized_name_preserves_unicode_and_rejects_controls() {
        assert_eq!(normalize_new_pet_name("  小橘  "), Ok("小橘".into()));
        assert_eq!(
            normalize_new_pet_name(&"寵".repeat(32)),
            Ok("寵".repeat(32))
        );
        for invalid in ["", "  ", "小\0橘", "小\n橘", "小\t橘", "\u{7f}"] {
            assert!(normalize_new_pet_name(invalid).is_err(), "{invalid:?}");
        }
        assert!(normalize_new_pet_name(&"寵".repeat(33)).is_err());
    }
}
