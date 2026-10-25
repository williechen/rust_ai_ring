use analytics_persistence::EventSummary;
use sqlx::PgPool;

use crate::{EventInput, valid_event};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistOutcome {
    Stored,
    AlreadyExists,
    IdentityConflict,
}

#[derive(Debug)]
pub enum PersistError {
    Invalid,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for PersistError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

async fn lookup_existing_event(
    pool: &PgPool, // original generic: <'_, sqlx::Postgres>,
    source_app: &str,
    event_id: &str,
) -> Result<(i16, String), sqlx::Error> {
    sqlx::query_as("SELECT schema_version, event_type FROM product_events WHERE source_app = $1 AND event_id = $2")
        .bind(source_app)
        .bind(event_id)
        .fetch_one(pool).await
}

pub async fn persist_event(
    pool: &PgPool,
    event: &EventInput,
) -> Result<PersistOutcome, PersistError> {
    if !valid_event(event) {
        return Err(PersistError::Invalid);
    }
    let event_id = event.event_id.to_string();

    let inserted = sqlx::query(
        "INSERT INTO product_events (source_app, event_id, schema_version, event_type)
     VALUES ($1, $2, $3, $4)
     ON CONFLICT (source_app, event_id) DO NOTHING",
    )
    .bind(&event.source_app)
    .bind(&event_id)
    .bind(i16::try_from(event.schema_version).expect("schema v1"))
    .bind(&event.event_type)
    .execute(pool)
    .await?
    .rows_affected();

    let outcome = if inserted == 1 {
        PersistOutcome::Stored
    } else {
        let previous = lookup_existing_event(pool, &event.source_app, &event_id).await?;
        if previous.0 == event.schema_version as i16 && previous.1 == event.event_type {
            PersistOutcome::AlreadyExists
        } else {
            PersistOutcome::IdentityConflict
        }
    };

    Ok(outcome)
}

pub async fn summary_for_source(
    pool: &PgPool,
    source: &str,
) -> Result<Vec<EventSummary>, sqlx::Error> {
    // 來源值一律綁定；同一 SQL statement 取得一致的聚合快照。
    sqlx::query_as::<_, EventSummary>(
        "SELECT source_app, event_type, COUNT(*)::bigint AS total
         FROM product_events
         WHERE source_app = $1
         GROUP BY source_app, event_type
         ORDER BY event_type",
    )
    .bind(source)
    .fetch_all(pool)
    .await
}
