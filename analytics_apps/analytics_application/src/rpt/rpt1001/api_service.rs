use std::future::Future;

/// Application 的報表頁面切片；不依賴 HTTP、HTML 或資料庫。
#[derive(Debug, PartialEq, Eq)]
pub struct PageWindow {
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
    pub offset: usize,
}

impl PageWindow {
    pub fn from_count(total: usize, requested_page: usize, requested_size: usize) -> Self {
        let page_size = requested_size.max(1);
        let total_pages = total.saturating_sub(1) / page_size + 1;
        let page = requested_page.clamp(1, total_pages);
        Self {
            page,
            page_size,
            total_pages,
            offset: (page - 1) * page_size,
        }
    }
}

fn csv_cell(value: &str) -> String {
    // 試算表可能把前置運算符號解讀為公式。
    let safe = if value
        .chars()
        .next()
        .is_some_and(|c| matches!(c, '=' | '+' | '-' | '@' | '\t' | '\r'))
    {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}

pub fn render_csv<'a>(rows: impl IntoIterator<Item = (&'a str, &'a str, i64)>) -> String {
    let mut csv = String::from("\u{feff}source_app,event_type,total\r\n");
    for (source_app, event_type, total) in rows {
        csv.push_str(&format!(
            "{},{},{}\r\n",
            csv_cell(source_app),
            csv_cell(event_type),
            total,
        ));
    }
    csv
}

pub fn render_daily(source: &str, kind: &str, report: &crate::daily_report::DailyReport) -> String {
    let mut csv = String::from(
        "\u{feff}source_app,event_type,timezone,start_inclusive,end_exclusive,date,total,unknown_dates\r\n",
    );
    let source = csv_cell(source);
    let kind = csv_cell(kind);
    let all_dates = report.requested_start.is_none() && report.requested_end.is_none();
    let unknown_dates = if all_dates {
        report.unknown_dates.to_string()
    } else {
        String::new()
    };
    for (date, total) in &report.rows {
        csv.push_str(&format!(
            "{source},{kind},\"UTC\",{},{},{},{total},{unknown_dates}\r\n",
            csv_cell(&report.start),
            csv_cell(&report.end),
            csv_cell(date),
        ));
    }
    if all_dates && report.rows.is_empty() && report.unknown_dates > 0 {
        // 沒有可分日的資料時，日期與每日總數留空，只輸出未知日期筆數。
        csv.push_str(&format!("{source},{kind},\"UTC\",,,,,{unknown_dates}\r\n"));
    }
    csv
}

pub struct SummaryRow {
    pub source_app: String,
    pub event_type: String,
    pub total: i64,
}

pub trait SummaryStore {
    type Error;
    fn read(
        &self,
        source: &str,
        kind: &str,
    ) -> impl Future<Output = Result<Vec<SummaryRow>, Self::Error>> + Send;
}

pub async fn csv<S: SummaryStore>(store: &S, source: &str, kind: &str) -> Result<String, S::Error> {
    let rows = store.read(source, kind).await?;
    Ok(crate::report_csv::render_csv(rows.iter().map(|r| {
        (r.source_app.as_str(), r.event_type.as_str(), r.total)
    })))
}

pub async fn read<S: SummaryStore>(
    store: &S,
    source: &str,
    kind: &str,
) -> Result<Vec<SummaryRow>, S::Error> {
    store.read(source, kind).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_and_out_of_range_pages_clamp() {
        assert_eq!(
            PageWindow::from_count(0, 999, 25),
            PageWindow {
                page: 1,
                page_size: 25,
                total_pages: 1,
                offset: 0
            }
        );
        assert_eq!(
            PageWindow::from_count(27, 999, 25),
            PageWindow {
                page: 2,
                page_size: 25,
                total_pages: 2,
                offset: 25
            }
        );
        assert_eq!(PageWindow::from_count(27, 0, 25).offset, 0);
        assert_eq!(PageWindow::from_count(25, 2, 25).total_pages, 1);
        assert_eq!(
            PageWindow::from_count(usize::MAX, usize::MAX, 1).offset,
            usize::MAX - 1
        );
    }
}

#[cfg(test)]
mod csv_contract_tests {
    #[test]
    fn daily_csv_keeps_dates_zero_days_unknown_count_and_formula_safety() {
        let r = crate::daily_report::DailyReport {
            requested_start: None,
            requested_end: None,
            start: "2026-11-01".into(),
            end: "2026-11-03".into(),
            rows: vec![("2026-11-01".into(), 2), ("2026-11-02".into(), 0)],
            unknown_dates: 3,
        };
        let csv = super::render_daily("web_chat", "=1+1", &r);
        assert!(csv.starts_with("\u{feff}source_app,event_type,timezone,start_inclusive,end_exclusive,date,total,unknown_dates\r\n"));
        assert!(csv.contains("\"2026-11-02\",0,3\r\n"));
        assert!(csv.contains("\"'=1+1\""));
        assert!(csv.contains("\"UTC\""));
    }

    #[test]
    fn daily_csv_only_includes_unknown_count_when_both_requested_bounds_are_absent() {
        let mut report = crate::daily_report::DailyReport {
            requested_start: None,
            requested_end: None,
            start: "2026-11-01".into(),
            end: "2026-11-03".into(),
            rows: vec![],
            unknown_dates: 3,
        };
        let render = |report: &crate::daily_report::DailyReport| {
            super::render_daily("web_chat", "message", report)
        };
        let unknown_only = render(&report);
        assert_eq!(
            unknown_only.lines().nth(1),
            Some("\"web_chat\",\"message\",\"UTC\",,,,,3")
        );
        report.unknown_dates = 0;
        assert_eq!(render(&report).lines().count(), 1);
        report.unknown_dates = 3;

        for (start, end) in [
            (Some("2026-11-01"), None),
            (None, Some("2026-11-03")),
            (Some("2026-11-01"), Some("2026-11-03")),
        ] {
            report.requested_start = start.map(str::to_owned);
            report.requested_end = end.map(str::to_owned);
            assert_eq!(render(&report).lines().count(), 1);

            report.rows.push(("2026-11-02".into(), 2));
            let bounded = render(&report);
            assert_eq!(bounded.lines().count(), 2);
            assert!(
                bounded
                    .lines()
                    .nth(1)
                    .unwrap()
                    .ends_with("\"2026-11-02\",2,")
            );
            report.rows.clear();
        }
    }
}
