#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DailyReport {
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
