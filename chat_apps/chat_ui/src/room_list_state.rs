pub(super) fn filter_rooms(rows: &[(String, String)], query: &str) -> Vec<(String, String)> {
    let needle = query.trim().to_lowercase();
    rows.iter()
        .filter(|(_, name)| name.to_lowercase().contains(&needle))
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
    previous: Vec<(String, String)>,
    previous_query: String,
    result: Result<Vec<(String, String)>, String>,
) -> (Vec<(String, String)>, String, String) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_preserves_ids_and_handles_all_original_queries() {
        let rooms = vec![
            ("room_1".into(), "Rust 技術交流".into()),
            ("room_2".into(), "生活閒聊".into()),
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
        let (rows, query, status) = refresh_result_view(
            vec![("old".into(), "舊房間".into())],
            "old".into(),
            Ok(vec![]),
        );
        assert!(rows.is_empty());
        assert_eq!(query, "");
        assert_eq!(status, "尚無房間");
    }

    #[test]
    fn failures_preserve_complete_snapshot_query_and_retry_message() {
        for (old, error, previous_query) in [
            (
                vec![
                    ("one".into(), "同名房間".into()),
                    ("two".into(), "同名房間".into()),
                ],
                "讀取失敗：503",
                "同名",
            ),
            (
                vec![("room_1".into(), "Rust 技術交流".into())],
                "503",
                "Rust",
            ),
            (vec![("a".into(), "Rust".into())], "503", "Rust"),
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
            vec![("room_2".into(), "生活閒聊".into())],
            vec![("a".into(), "Rust".into()), ("b".into(), "Pet".into())],
        ] {
            let (rows, query, status) = refresh_result_view(
                vec![("old".into(), "old".into())],
                "Rust".into(),
                Ok(fresh.clone()),
            );
            assert_eq!(rows, fresh);
            assert_eq!(rows[0].0, fresh[0].0);
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
}
