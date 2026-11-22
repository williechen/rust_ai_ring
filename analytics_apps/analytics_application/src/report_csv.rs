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

#[cfg(test)]
mod day65_tests {
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
