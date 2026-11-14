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
