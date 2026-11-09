#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RoomListItem {
    pub id: String,
    pub name: String,
    pub category_label: Option<String>,
}

#[cfg(test)]
pub(crate) fn test_room(id: &str, name: &str) -> RoomListItem {
    RoomListItem {
        id: id.into(),
        name: name.into(),
        category_label: None,
    }
}

pub(super) fn filter_rooms(rows: &[RoomListItem], query: &str) -> Vec<RoomListItem> {
    let needle = query.trim().to_lowercase();
    rows.iter()
        .filter(|room| room.name.to_lowercase().contains(&needle))
        .cloned()
        .collect()
}

pub(super) fn room_list_feedback(loaded: bool, total: usize, visible: usize) -> &'static str {
    if !loaded {
        "請先讀取房間列表"
    } else if total == 0 {
        "尚無房間"
    } else if visible == 0 {
        "沒有符合條件的房間"
    } else {
        "請選擇房間"
    }
}

#[cfg(any(target_arch = "wasm32", test))]
pub(super) fn refresh_result_view(
    previous: Vec<RoomListItem>,
    previous_query: String,
    result: Result<Vec<RoomListItem>, String>,
) -> (Vec<RoomListItem>, String, String) {
    match result {
        Ok(next) => {
            let status = if next.is_empty() {
                "尚無房間"
            } else {
                "請選擇房間"
            };
            (next, String::new(), status.to_owned())
        }
        Err(error) => (previous, previous_query, format!("{error}，請重試")),
    }
}

/// chat_apps/chat_ui/src/room_list_state.rs
#[cfg(any(target_arch = "wasm32", test))]
pub(super) fn should_auto_load(has_snapshot: bool, loading: bool) -> bool {
    !has_snapshot && !loading
}

/// Retains loading/error messages while deriving settled list feedback.
pub(super) fn room_status_text(status: &str, loaded: bool, total: usize, visible: usize) -> String {
    if matches!(status, "查詢中…" | "載入中…") || status.ends_with("，請重試") {
        return status.to_owned();
    }
    room_list_feedback(loaded, total, visible).to_owned()
}

/// Reset the local search without discarding the last successful room snapshot.
pub(super) fn clear_room_search(loaded: bool, rows: &[RoomListItem]) -> (String, String) {
    (
        String::new(),
        room_list_feedback(loaded, rows.len(), rows.len()).to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_preserves_ids_and_handles_all_original_queries() {
        let rooms = vec![
            test_room("room_1", "Rust 技術交流"),
            test_room("room_2", "生活閒聊"),
        ];
        for query in [" 技術 ", "技術", "RUST"] {
            assert_eq!(
                filter_rooms(&rooms, query),
                vec![rooms[0].clone()],
                "{query:?}"
            );
        }
        for query in ["", " "] {
            assert_eq!(filter_rooms(&rooms, query), rooms, "{query:?}");
        }
        assert!(filter_rooms(&rooms, "不存在").is_empty());
    }

    #[test]
    fn empty_success_replaces_rows_and_clears_query() {
        let (rows, query, status) =
            refresh_result_view(vec![test_room("old", "舊房間")], "old".into(), Ok(vec![]));
        assert!(rows.is_empty());
        assert_eq!(query, "");
        assert_eq!(status, "尚無房間");
    }

    #[test]
    fn failures_preserve_complete_snapshot_query_and_retry_message() {
        for (old, error, previous_query) in [
            (
                vec![test_room("one", "同名房間"), test_room("two", "同名房間")],
                "讀取失敗：503",
                "同名",
            ),
            (vec![test_room("room_1", "Rust 技術交流")], "503", "Rust"),
            (vec![test_room("a", "Rust")], "503", "Rust"),
        ] {
            let (rows, query, status) =
                refresh_result_view(old.clone(), previous_query.into(), Err(error.into()));
            assert_eq!(rows, old);
            assert_eq!(query, previous_query);
            assert_eq!(status, format!("{error}，請重試"));
            assert!(status.contains("請重試"));
        }
    }

    #[test]
    fn success_replaces_links_and_returns_full_snapshot() {
        for fresh in [
            vec![test_room("room_2", "生活閒聊")],
            vec![test_room("a", "Rust"), test_room("b", "Pet")],
        ] {
            let (rows, query, status) = refresh_result_view(
                vec![test_room("old", "old")],
                "Rust".into(),
                Ok(fresh.clone()),
            );
            assert_eq!(rows, fresh);
            assert_eq!(rows[0].id, fresh[0].id);
            assert_eq!(query, "");
            assert_eq!(status, "請選擇房間");
            assert_eq!(filter_rooms(&rows, &query).len(), fresh.len());
        }
    }

    #[test]
    fn distinguish_not_loaded_empty_filtered_and_found() {
        assert_eq!(room_list_feedback(false, 0, 0), "請先讀取房間列表");
        assert_eq!(room_list_feedback(true, 0, 0), "尚無房間");
        assert_eq!(room_list_feedback(true, 3, 0), "沒有符合條件的房間");
        assert_eq!(room_list_feedback(true, 3, 2), "請選擇房間");
    }

    #[test]
    fn day39_initial_load_is_once_and_does_not_interrupt_pending_request() {
        assert!(should_auto_load(false, false));
        assert!(!should_auto_load(false, true));
        assert!(!should_auto_load(true, false));
        assert!(!should_auto_load(true, true));
    }
}

#[cfg(test)]
mod day43_clear_room_search_tests {
    use super::*;
    #[test]
    fn clear_restores_all_visible_rooms_without_discarding_snapshot() {
        let rows = vec![test_room("a", "Rust"), test_room("b", "Pet")];
        assert_eq!(filter_rooms(&rows, "Rust").len(), 1);
        let (query, feedback) = clear_room_search(true, &rows);
        assert_eq!(query, "");
        assert_eq!(feedback, "請選擇房間");
        assert_eq!(filter_rooms(&rows, &query), rows);
        assert_eq!(clear_room_search(false, &[]).1, "請先讀取房間列表");
    }
}

#[cfg(test)]
mod day42_status_refactor_tests {
    use super::room_status_text;
    #[test]
    fn loading_error_empty_and_found_contract_survives_move() {
        assert_eq!(room_status_text("載入中…", false, 0, 0), "載入中…");
        assert_eq!(room_status_text("503，請重試", true, 3, 1), "503，請重試");
        assert_eq!(room_status_text("", true, 0, 0), "尚無房間");
        assert_eq!(room_status_text("", true, 3, 0), "沒有符合條件的房間");
        assert_eq!(room_status_text("", true, 3, 2), "請選擇房間");
    }
}
