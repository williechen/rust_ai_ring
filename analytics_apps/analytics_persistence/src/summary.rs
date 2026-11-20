use analytics_application::summary::{SummaryRow, SummaryStore};
pub struct PgSummary(pub sqlx::PgPool);
impl SummaryStore for PgSummary {
    type Error = sqlx::Error;

    async fn read(&self, source: &str, kind: &str) -> Result<Vec<SummaryRow>, Self::Error> {
        let rows = if kind.is_empty() {
            crate::summary_for_source(&self.0, source).await?
        } else {
            crate::summary_for_source_and_type(&self.0, source, kind).await?
        };
        Ok(rows
            .into_iter()
            .map(|r| SummaryRow {
                source_app: r.source_app,
                event_type: r.event_type,
                total: r.total,
            })
            .collect())
    }
}

/// 日期與文字條件由同一查詢傳入；空白日期不加入限制。
pub struct PgFilteredSummary {
    pub pool: sqlx::PgPool,
    pub start: Option<String>,
    pub end: Option<String>,
}
impl SummaryStore for PgFilteredSummary {
    type Error = crate::daily_report::DailyError;
    async fn read(&self, source: &str, kind: &str) -> Result<Vec<SummaryRow>, Self::Error> {
        let start = self
            .start
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty());
        let end = self.end.as_deref().map(str::trim).filter(|v| !v.is_empty());
        if start.is_some_and(|v| !analytics_application::daily_report::date_shape(v))
            || end.is_some_and(|v| !analytics_application::daily_report::date_shape(v))
        {
            return Err(crate::daily_report::DailyError::Invalid);
        }
        let valid: bool = sqlx::query_scalar("SELECT COALESCE($1::date <= $2::date,true)")
            .bind(start)
            .bind(end)
            .fetch_one(&self.pool)
            .await?;
        if !valid {
            return Err(crate::daily_report::DailyError::Invalid);
        }
        let rows=sqlx::query_as::<_,crate::EventSummary>(r#"SELECT p.source_app,c.wire_value AS event_type,COUNT(*)::bigint AS total
            FROM product_events p JOIN codemap c ON c.category='event_type' AND c.code=p.event_type
            WHERE p.source_app=$1 AND ($2='' OR c.wire_value ILIKE $2 ESCAPE '\' OR c.value ILIKE $2 ESCAPE '\')
            AND ($3::date IS NULL OR p.occurred_at>=($3::date::timestamp AT TIME ZONE 'UTC'))
            AND ($4::date IS NULL OR p.occurred_at<($4::date::timestamp AT TIME ZONE 'UTC'))
            GROUP BY p.source_app,c.wire_value ORDER BY c.wire_value COLLATE "default""#)
            .bind(source).bind(crate::search_text::pattern(kind)).bind(start).bind(end).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| SummaryRow {
                source_app: r.source_app,
                event_type: r.event_type,
                total: r.total,
            })
            .collect())
    }
}
