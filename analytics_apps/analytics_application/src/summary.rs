use std::future::Future;

pub struct SummaryRow {
    pub source_app: String,
    pub event_type: String,
    pub total: i64,
}

pub trait SummaryStore {
    type Error;
    fn read(
        &self,
        source: &str,
        kind: &str,
    ) -> impl Future<Output = Result<Vec<SummaryRow>, Self::Error>> + Send;
}

pub async fn csv<S: SummaryStore>(store: &S, source: &str, kind: &str) -> Result<String, S::Error> {
    let rows = store.read(source, kind).await?;
    Ok(crate::report_csv::render_csv(rows.iter().map(|r| {
        (r.source_app.as_str(), r.event_type.as_str(), r.total)
    })))
}
