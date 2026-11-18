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
