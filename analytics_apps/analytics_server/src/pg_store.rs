use analytics_persistence::summary_for_source;
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
        sqlx::query_as(
        "SELECT p.schema_version, c.wire_value
         FROM product_events AS p
         JOIN codemap AS c ON c.category = 'event_type' AND c.code = p.event_type
         WHERE p.source_app = $1 AND p.event_id = $2",
    )
    .bind(source_app)
    .bind(event_id)
    .fetch_one(pool)
    .await
}

pub async fn persist_event(
    pool: &PgPool,
    event: &EventInput,
) -> Result<PersistOutcome, PersistError> {
    if !valid_event(event) {
        return Err(PersistError::Invalid);
    }
    let event_id = event.event_id.to_string();
    let mut tx = pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO product_events (source_app, event_id, schema_version, event_type)
     VALUES ($1, $2, $3, $4)
     ON CONFLICT (source_app, event_id) DO NOTHING",
    )
    .bind(&event.source_app)
    .bind(&event_id)
    .bind(i16::try_from(event.schema_version).expect("schema v1"))
    .bind(&event.event_type)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    let outcome = if inserted == 1 {
        tx.commit().await?;
        PersistOutcome::Stored
    } else {
        // 未提交的事件不可留下新的字典映射。
        tx.rollback().await?;
        let previous = lookup_existing_event(pool, &event.source_app, &event_id).await?;
        if previous.0 == event.schema_version as i16 && previous.1 == event.event_type {
            PersistOutcome::AlreadyExists
        } else {
            PersistOutcome::IdentityConflict
        }
    };

    Ok(outcome)
}
