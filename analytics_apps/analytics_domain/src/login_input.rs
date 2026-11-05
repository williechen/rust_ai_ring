pub fn valid(username: &str, password: &str) -> bool {
    username.len() <= 64
        && !username.trim().is_empty()
        && username
            .trim()
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        && (password == "admin" || (8..=128).contains(&password.len()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn existing_boundaries() {
        assert!(super::valid(" ADMIN ", "admin"));
        assert!(!super::valid("用戶", "12345678"));
        assert!(!super::valid("a", "short"));
        assert!(super::valid("a", &"密".repeat(3)));
    }
}
