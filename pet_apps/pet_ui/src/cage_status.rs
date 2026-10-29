use std::collections::HashMap;

/// Store the result under the queried UUID only; an IPC failure is not false.
pub(crate) fn record_cage_status(
    statuses: &mut HashMap<String, String>,
    id: &str,
    result: Result<bool, String>,
) {
    let text = match result {
        Ok(true) => "已回籠".to_owned(),
        Ok(false) => "已出籠".to_owned(),
        Err(error) => format!("查詢回籠狀態失敗：{error}"),
    };
    statuses.insert(id.to_owned(), text);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_pets_keep_independent_status_and_failure_is_not_false() {
        let mut statuses = HashMap::new();
        record_cage_status(&mut statuses, "pet-a", Ok(false));
        record_cage_status(&mut statuses, "pet-b", Ok(true));
        assert_eq!(statuses["pet-a"], "已出籠");
        assert_eq!(statuses["pet-b"], "已回籠");
        record_cage_status(&mut statuses, "pet-a", Err("SQLite read failed".into()));
        assert!(statuses["pet-a"].contains("查詢回籠狀態失敗"));
        assert_eq!(statuses["pet-b"], "已回籠");
    }
}
