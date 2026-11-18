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
    for (date, total) in &report.rows {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{}\r\n",
            csv_cell(source),
            csv_cell(kind),
            csv_cell("UTC"),
            csv_cell(&report.start),
            csv_cell(&report.end),
            csv_cell(date),
            total,
            report.unknown_dates
        ));
    }
    csv
}

#[cfg(test)]
mod day65_tests {
    #[test]
    fn daily_csv_keeps_dates_zero_days_unknown_count_and_formula_safety() {
        let r = crate::daily_report::DailyReport {
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
}
