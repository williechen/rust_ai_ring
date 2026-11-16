#![cfg(feature = "postgres-integration")]
use analytics_server::{
    EventInput,
    pg_store::{PersistError, PersistOutcome, persist_event},
};
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test(migrations = "../migrations")]
async fn postgres_event_idempotency(pool: PgPool) {
    let id = Uuid::new_v4();
    let event = EventInput {
        occurred_at: None,
        schema_version: 1,
        event_id: id,
        source_app: "web_chat".into(),
        event_type: "chat.message.persisted".into(),
    };
    assert_eq!(
        persist_event(&pool, &event).await.unwrap(),
        PersistOutcome::Stored
    );
    assert_eq!(
        persist_event(&pool, &event).await.unwrap(),
        PersistOutcome::AlreadyExists
    );
    let conflict = EventInput {
        occurred_at: None,
        schema_version: 1,
        event_id: id,
        source_app: "web_chat".into(),
        event_type: "chat.message.changed".into(),
    };
    assert_eq!(
        persist_event(&pool, &conflict).await.unwrap(),
        PersistOutcome::IdentityConflict
    );
    let pet = EventInput {
        occurred_at: None,
        schema_version: 1,
        event_id: id,
        source_app: "desktop_pet".into(),
        event_type: "pet.cleanup.succeeded".into(),
    };
    assert_eq!(
        persist_event(&pool, &pet).await.unwrap(),
        PersistOutcome::Stored
    );
    let invalid = EventInput {
        occurred_at: None,
        schema_version: 2,
        event_id: Uuid::new_v4(),
        source_app: "web_chat".into(),
        event_type: "chat.message.persisted".into(),
    };
    assert!(matches!(
        persist_event(&pool, &invalid).await,
        Err(PersistError::Invalid)
    ));
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM product_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 2);
}
