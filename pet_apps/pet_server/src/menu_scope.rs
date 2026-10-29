pub(crate) fn control_menu_pet_id(menu_id: &str) -> Option<String> {
    let id = menu_id
        .strip_prefix("pet:")?
        .strip_suffix(":open_control")?;
    uuid::Uuid::parse_str(id).ok().map(|id| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_control_menu_accepts_only_scoped_uuid() {
        let a = "00000000-0000-0000-0000-000000000001";
        let b = "00000000-0000-0000-0000-000000000002";
        assert_eq!(
            control_menu_pet_id(&format!("pet:{a}:open_control")),
            Some(a.into())
        );
        assert_eq!(
            control_menu_pet_id(&format!("pet:{b}:open_control")),
            Some(b.into())
        );
        for invalid in [
            "open_control",
            "pet:bad:open_control",
            "pet:00000000-0000-0000-0000-000000000001:sleep",
            "pet::open_control",
        ] {
            assert_eq!(control_menu_pet_id(invalid), None);
        }
    }
}
