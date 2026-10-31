pub use crate::csv::render_csv;
use crate::{IngestState, password_login::require_source_session};
use analytics_persistence::{summary_for_source, summary_for_source_and_type};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};

#[derive(Default, serde::Deserialize)]
pub struct ExportQuery {
    #[serde(default)]
    event_type: String,
}

/// Human Session is required; machine Bearer tokens never authorize CSV export.
pub async fn export_summary(
    State(state): State<IngestState>,
    Path(source): Path<String>,
    Query(query): Query<ExportQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let raw = require_source_session(&state, &headers, &source).await?;
    if !matches!(source.as_str(), "web_chat" | "desktop_pet") {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !query.event_type.is_empty() && !crate::valid_event_type(&query.event_type) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let rows = if query.event_type.is_empty() {
        summary_for_source(&state.pool, &source).await
    } else {
        summary_for_source_and_type(&state.pool, &source, &query.event_type).await
    }
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
