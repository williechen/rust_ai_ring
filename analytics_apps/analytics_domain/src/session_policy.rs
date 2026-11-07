/// 管理工作階段的最小/最大閒置秒數。
pub fn valid_idle_seconds(seconds: i64) -> bool {
    (60..=86_400).contains(&seconds)
}

#[cfg(test)]
mod tests {
    use super::valid_idle_seconds;
    #[test]
    fn accepts_only_bounded_seconds() {
        for bad in [0, 59, 86_401] {
            assert!(!valid_idle_seconds(bad));
        }
        for ok in [60, 300, 86_400] {
            assert!(valid_idle_seconds(ok));
        }
    }
}
