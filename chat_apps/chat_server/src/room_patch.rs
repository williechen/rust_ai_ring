/// 僅允許規範化 UUID 房間路徑；不接受 query、片段或多餘 path segment。
pub(crate) fn canonical_room_id(path: &str) -> Option<String> {
    let raw = path.strip_prefix("/room/")?;
    let id = uuid::Uuid::parse_str(raw).ok()?;
    (id.to_string() == raw).then(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_room_path_is_fail_closed() {
        let id = "00000000-0000-0000-0000-000000000001";
        assert_eq!(canonical_room_id(&format!("/room/{id}")), Some(id.into()));
        for invalid in [
            "/room/",
            "/room/not-a-uuid",
            "/room/00000000000000000000000000000001",
            "/room/00000000-0000-0000-0000-000000000001/extra",
            "/room/00000000-0000-0000-0000-000000000001?x=1",
        ] {
            assert_eq!(canonical_room_id(invalid), None, "{invalid}");
        }
    }
}
