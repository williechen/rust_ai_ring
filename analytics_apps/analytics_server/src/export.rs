use crate::IngestState;
use analytics_persistence::EventSummary;
use analytics_persistence::summary_for_source;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};

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

/// Human Session is required; machine Bearer tokens never authorize CSV export.
pub async fn export_summary(
    State(state): State<IngestState>,
    Path(source): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let raw = crate::password_login::require_query_session(&state, &headers).await?;
    if !matches!(source.as_str(), "web_chat" | "desktop_pet") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let rows = summary_for_source(&state.pool, &source)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut response = render_csv(&rows).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        "text/csv; charset=utf-8".parse().expect("static MIME"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"analytics-{source}-summary.csv\"")
            .parse()
            .expect("source allowlist"),
    );
    Ok(crate::password_login::renewed(response, &raw))
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
