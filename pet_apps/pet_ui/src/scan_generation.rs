/// 非同步掃描結果僅在仍是目前請求時才能覆蓋畫面。
pub(crate) fn accept_scan_result(request_generation: u64, current_generation: u64) -> bool {
    request_generation == current_generation
}

#[cfg(test)]
mod tests {
    use super::accept_scan_result;
    #[test]
    fn canceled_or_superseded_scan_cannot_restore_preview() {
        assert!(accept_scan_result(3, 3));
        assert!(!accept_scan_result(2, 3));
        assert!(!accept_scan_result(3, 4));
    }
}
