#![cfg(feature = "postgres-integration")]

use analytics_persistence::summary_for_source_and_type;
use analytics_server::{
    EventInput,
    pg_store::{PersistOutcome, persist_event},
};
use sqlx::{PgPool, migrate::Migrator};
use uuid::Uuid;

const LAST: i64 = 20261107060000;
const LEGACY: &str = "chat.舊事件 & 查詢";

#[sqlx::test(migrations = false)]
async fn migration_keeps_old_ledger_and_protocol_values_without_storing_display_names(
    pool: PgPool,
) {
    let all = sqlx::migrate!("../migrations");
    let previous =
        Migrator::with_migrations(all.iter().filter(|m| m.version <= LAST).cloned().collect());
    previous.run(&pool).await.unwrap();
    let old: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO product_events(source_app,event_id,schema_version,event_type,received_at)
        VALUES('web_chat',$1,1,$2,'2026-10-01T02:03:04Z')",
    )
    .bind(&id)
    .bind(LEGACY)
    .execute(&pool)
    .await
    .unwrap();
    all.run(&pool).await.unwrap();
    all.run(&pool).await.unwrap();
    let stored: (String, String, i64) = sqlx::query_as(
        "SELECT event_id,event_type,extract(epoch FROM received_at)::bigint FROM product_events",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_ne!(stored.1, LEGACY, "業務表不得保存含中文的協定值");
    assert!(stored.1.starts_with("wire-sha256-"));
    assert_eq!(stored.0, id);
    assert_eq!(stored.2, 1790820184);
    let rows = summary_for_source_and_type(&pool, "web_chat", LEGACY)
        .await
        .unwrap();
    assert_eq!((&*rows[0].event_type, rows[0].total), (LEGACY, 1));
    let current: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(&current[..old.len()], old);
    assert_eq!(current.len(), old.len() + 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn live_ingestion_preserves_dedupe_and_failed_aliases_do_not_commit(pool: PgPool) {
    let event = EventInput {
        source_app: "web_chat".into(),
        event_id: Uuid::new_v4(),
        schema_version: 1,
        event_type: LEGACY.into(),
    };
    assert_eq!(
        persist_event(&pool, &event).await.unwrap(),
        PersistOutcome::Stored
    );
    assert_eq!(
        persist_event(&pool, &event).await.unwrap(),
        PersistOutcome::AlreadyExists
    );
    let code: String = sqlx::query_scalar("SELECT event_type FROM product_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(code, LEGACY, "即時寫入也必須存英文 key");
    let conflict = EventInput {
        event_type: code.clone(),
        ..event
    };
    assert_eq!(
        persist_event(&pool, &conflict).await.unwrap(),
        PersistOutcome::IdentityConflict
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM codemap WHERE category='event_type' AND wire_value=$1",
    )
    .bind(&code)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
    let competing = EventInput {
        event_id: Uuid::new_v4(),
        ..conflict
    };
    let (left, right) = tokio::join!(
        persist_event(&pool, &competing),
        persist_event(&pool, &competing)
    );
    let outcomes = [left.unwrap(), right.unwrap()];
    assert!(outcomes.contains(&PersistOutcome::Stored));
    assert!(outcomes.contains(&PersistOutcome::AlreadyExists));
    let second: String =
        sqlx::query_scalar("SELECT event_type FROM product_events WHERE event_id=$1")
            .bind(competing.event_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(code, second);
    assert_eq!(
        summary_for_source_and_type(&pool, "web_chat", &code)
            .await
            .unwrap()[0]
            .total,
        1
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn referenced_dictionary_codes_are_scoped_and_rename_in_place(pool: PgPool) {
    let event = EventInput {
        source_app: "desktop_pet".into(),
        event_id: Uuid::new_v4(),
        schema_version: 1,
        event_type: "pet.sleep".into(),
    };
    assert_eq!(
        persist_event(&pool, &event).await.unwrap(),
        PersistOutcome::Stored
    );
    let exists: Option<String> = sqlx::query_scalar("SELECT to_regclass('codemap')::text")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(exists.is_some(), "Analytics 須有來源與事件字典");
    sqlx::query(
        "UPDATE codemap SET value='寵物進入休息' WHERE category='event_type' AND code='pet.sleep'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        sqlx::query("DELETE FROM codemap WHERE category='event_type' AND code='pet.sleep'")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM codemap WHERE category='source_app' AND code='desktop_pet'")
            .execute(&pool)
            .await
            .is_err()
    );
    let stored: (String, String) =
        sqlx::query_as("SELECT source_app,event_type FROM product_events")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, ("desktop_pet".into(), "pet.sleep".into()));
}