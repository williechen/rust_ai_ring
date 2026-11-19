pub use crate::csv::render_csv;
use crate::{IngestState, password_login::require_source_session};

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
    Query(mut query): Query<ExportQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    query.event_type = query.event_type.trim().to_owned();
    let (raw, seconds) = require_source_session(&state, &headers, &source).await?;
    if !matches!(source.as_str(), "web_chat" | "desktop_pet") {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !query.event_type.is_empty() && !crate::valid_event_type(&query.event_type) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let csv = analytics_application::summary::csv(
        &analytics_persistence::summary::PgSummary(state.pool.clone()),
        &source,
        &query.event_type,
    )
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut response = csv.into_response();
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
    Ok(crate::password_login::renewed(response, &raw, seconds))
}

#[derive(Default, serde::Deserialize)]
pub struct DailyQuery {
    #[serde(default)]
    event_type: String,
    start: Option<String>,
    end: Option<String>,
}
pub async fn export_daily(
    State(state): State<IngestState>,
    Path(source): Path<String>,
    Query(mut query): Query<DailyQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    query.event_type = query.event_type.trim().to_owned();
    let (raw, seconds) = require_source_session(&state, &headers, &source).await?;
    if !matches!(source.as_str(), "web_chat" | "desktop_pet")
        || (!query.event_type.is_empty() && !crate::valid_event_type(&query.event_type))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let report = analytics_application::daily_report::load(
        &analytics_persistence::daily_report::PgDailyReport(state.pool.clone()),
        &source,
        &query.event_type,
        query.start.as_deref(),
        query.end.as_deref(),
    )
    .await
    .map_err(|e| match e {
        analytics_persistence::daily_report::DailyError::Invalid => StatusCode::BAD_REQUEST,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    })?;
    let mut response =
        analytics_application::report_csv::render_daily(&source, &query.event_type, &report)
            .into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        "text/csv; charset=utf-8".parse().unwrap(),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"analytics-{source}-daily.csv\"")
            .parse()
            .unwrap(),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    Ok(crate::password_login::renewed(response, &raw, seconds))
}
