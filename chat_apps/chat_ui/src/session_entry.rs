#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum SessionEntry {
    Checking,
    SignedOut,
    SignedIn,
    Unavailable,
}

pub(super) fn session_entry_display(
    state: SessionEntry,
) -> (&'static str, &'static str, &'static str) {
    match state {
        SessionEntry::Checking => ("/signin", "登入", "確認登入狀態中"),
        SessionEntry::SignedOut => ("/signin", "登入", ""),
        SessionEntry::SignedIn => ("/account", "管理", ""),
        SessionEntry::Unavailable => ("/signin", "登入", "無法確認登入狀態；可使用登入入口"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_navigation_is_fail_closed_and_preserves_existing_contract() {
        assert_eq!(session_entry_display(SessionEntry::Checking).0, "/signin");
        assert_eq!(session_entry_display(SessionEntry::SignedOut).1, "登入");
        assert_eq!(
            session_entry_display(SessionEntry::Unavailable).0,
            "/signin"
        );
        assert!(
            session_entry_display(SessionEntry::Unavailable)
                .2
                .contains("無法確認")
        );
        assert_eq!(
            session_entry_display(SessionEntry::SignedIn),
            ("/account", "管理", "")
        );
    }
}
