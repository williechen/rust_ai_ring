#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DailyReport {
    pub requested_start: Option<String>,
    pub requested_end: Option<String>,
    pub start: String,
    pub end: String,
    pub rows: Vec<(String, i64)>,
    pub unknown_dates: i64,
}

/// 僅允許日期的固定形狀；閏日及範圍仍由資料來源的日期型別驗證。
pub fn date_shape(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && (0..10)
            .filter(|i| ![4, 7].contains(i))
            .all(|i| b[i].is_ascii_digit())
}

pub trait DailyStore: Sync {
    type Error;
    fn recent_week(
        &self,
    ) -> impl std::future::Future<Output = Result<(String, String), Self::Error>> + Send;
    
    fn read(
        &self,
        source: &str,
        kind: &str,
        start: Option<&str>,
        end: Option<&str>,
    ) -> impl std::future::Future<Output = Result<DailyReport, Self::Error>> + Send;
}

pub async fn load<S: DailyStore>(
    store: &S,
    source: &str,
    kind: &str,
    start: Option<&str>,
    end: Option<&str>,
) -> Result<DailyReport, S::Error> {
    store.read(source, kind, start, end).await
}

pub fn day_window(report: &DailyReport, index: usize) -> Option<(&str, &str)> {
    let start = report.rows.get(index)?.0.as_str();
    let end = report
        .rows
        .get(index + 1)
        .map(|r| r.0.as_str())
        .unwrap_or(&report.end);
    Some((start, end))
}

/// 同一次讀模型同時決定每日CSV資料與日期範圍。
pub async fn csv<S: DailyStore>(
    store: &S,
    source: &str,
    kind: &str,
    start: Option<&str>,
    end: Option<&str>,
) -> Result<String, S::Error> {
    let report = load(store, source, kind, start, end).await?;
    Ok(crate::report_csv::render_daily(source, kind, &report))
}

pub async fn recent_week<S: DailyStore>(store: &S) -> Result<(String, String), S::Error> {
    store.recent_week().await
}

#[cfg(test)]
mod day66_tests {
    #[test]
    fn selected_day_uses_next_boundary_and_last_day_keeps_exclusive_end() {
        let r = super::DailyReport {
            requested_start: None,
            requested_end: None,
            start: "2026-11-08".into(),
            end: "2026-11-11".into(),
            rows: vec![
                ("2026-11-08".into(), 1),
                ("2026-11-09".into(), 2),
                ("2026-11-10".into(), 0),
            ],
            unknown_dates: 0,
        };
        assert_eq!(super::day_window(&r, 1), Some(("2026-11-09", "2026-11-10")));
        assert_eq!(super::day_window(&r, 2), Some(("2026-11-10", "2026-11-11")));
        assert_eq!(super::day_window(&r, 3), None);
    }
}
