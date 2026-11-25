pub fn checked(value: &str) -> Option<&str> {
    if value.len() > 4096 || value.chars().any(char::is_control) || value.contains(['\\', '#']) {
        return None;
    }
    (value == "/dashboard" || value.starts_with("/dashboard?")).then_some(value)
}

pub fn login_location(value: &str) -> String {
    let Some(value) = checked(value) else {
        return "/login".into();
    };
    let mut encoded = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(b));
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    format!("/login?next={encoded}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn sso1001_unit_001_only_internal_report_paths_are_accepted() {
        for path in [
            "https://evil.example",
            "//evil.example",
            "/dashboard/../login",
            "/dashboard#x",
            "/dashboard\\evil",
            "/dashboard?x=\n",
        ] {
            assert!(super::checked(path).is_none());
        }
        assert_eq!(
            super::checked("/dashboard?source=desktop_pet&start=&end=2026-11-01"),
            Some("/dashboard?source=desktop_pet&start=&end=2026-11-01")
        );
        assert!(super::login_location("/dashboard?event_type=%25_&start=").contains("%2525_"));
    }
}
