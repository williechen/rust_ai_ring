use analytics_persistence::EventSummary;

pub fn render_csv(rows: &[EventSummary]) -> String {
    analytics_application::report_csv::render_csv(
        rows.iter()
            .map(|row| (row.source_app.as_str(), row.event_type.as_str(), row.total)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_escapes_quotes_and_spreadsheet_formulas() {
        let csv = render_csv(&[
            EventSummary {
                source_app: "web_chat".into(),
                event_type: "=1+1".into(),
                total: 2,
            },
            EventSummary {
                source_app: "desktop_pet".into(),
                event_type: "say \"hi\",\nagain".into(),
                total: 1,
            },
        ]);
        assert!(csv.starts_with("\u{feff}source_app,event_type,total\r\n"));
        assert!(csv.contains("\"web_chat\",\"'=1+1\",2\r\n"));
        assert!(csv.contains("\"desktop_pet\",\"say \"\"hi\"\",\nagain\",1\r\n"));
    }
}
