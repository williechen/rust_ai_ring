use analytics_persistence::EventSummary;

fn csv_cell(value: &str) -> String {
    // Spreadsheet applications may interpret leading operators as formulas.
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

pub fn render_csv(rows: &[EventSummary]) -> String {
    let mut csv = String::from("\u{feff}source_app,event_type,total\r\n");
    for row in rows {
        csv.push_str(&format!(
            "{},{},{}\r\n",
            csv_cell(&row.source_app),
            csv_cell(&row.event_type),
            row.total,
        ));
    }
    csv
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
