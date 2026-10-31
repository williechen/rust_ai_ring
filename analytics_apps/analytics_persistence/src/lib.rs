pub mod login_session;

/// PostgreSQL 聚合查詢結果，供 JSON API 與 Leptos SSR 共用。
#[derive(Debug, serde::Serialize, sqlx::FromRow, PartialEq, Eq)]
pub struct EventSummary {
    pub source_app: String,
    pub event_type: String,
    pub total: i64,
}

pub async fn summary_for_source(
    pool: &sqlx::PgPool,
    source: &str,
) -> Result<Vec<EventSummary>, sqlx::Error> {
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

/// 僅聚合已提交的精確來源與事件類型；輸入不拼入 SQL。
pub async fn summary_for_source_and_type(
    pool: &sqlx::PgPool,
    source: &str,
    event_type: &str,
) -> Result<Vec<EventSummary>, sqlx::Error> {
    sqlx::query_as::<_, EventSummary>(
        "SELECT source_app, event_type, COUNT(*)::bigint AS total
         FROM product_events
         WHERE source_app = $1 AND event_type = $2
         GROUP BY source_app, event_type",
    )
    .bind(source)
    .bind(event_type)
    .fetch_all(pool)
    .await
}

#[cfg(all(test, feature = "postgres-integration"))]
mod day35_exact_kind_adapter_tests {
    use super::{EventSummary, summary_for_source_and_type};

    #[sqlx::test(migrations = "../migrations")]
    async fn exact_kind_counts_committed_rows_only_for_requested_source(pool: sqlx::PgPool) {
        for (source, kind) in [
            ("web_chat", "chat.message.persisted"),
            ("web_chat", "chat.message.persisted"),
            ("web_chat", "chat.room.created"),
            ("desktop_pet", "chat.message.persisted"),
        ] {
            sqlx::query(
                "INSERT INTO product_events (source_app,event_id,schema_version,event_type)
                 VALUES ($1,$2,1,$3)",
            )
            .bind(source)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(kind)
            .execute(&pool)
            .await
            .unwrap();
        }
        let chat = summary_for_source_and_type(&pool, "web_chat", "chat.message.persisted")
            .await
            .unwrap();
        assert_eq!(
            chat,
            vec![EventSummary {
                source_app: "web_chat".into(),
                event_type: "chat.message.persisted".into(),
                total: 2,
            }]
        );
        let pet = summary_for_source_and_type(&pool, "desktop_pet", "chat.message.persisted")
            .await
            .unwrap();
        assert_eq!(pet[0].total, 1);
        assert_eq!(pet.len(), 1);
        assert!(
            summary_for_source_and_type(&pool, "web_chat", "' OR TRUE --")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            summary_for_source_and_type(&pool, "web_chat", "not.recorded")
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn rolled_back_event_is_not_counted(pool: sqlx::PgPool) {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO product_events (source_app,event_id,schema_version,event_type)
             VALUES ('web_chat',$1,1,'chat.message.persisted')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.rollback().await.unwrap();
        assert!(
            summary_for_source_and_type(&pool, "web_chat", "chat.message.persisted")
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn all_kinds_are_sorted_and_scoped_to_one_source(pool: sqlx::PgPool) {
        for (source, kind) in [
            ("web_chat", "z.last"),
            ("web_chat", "a.first"),
            ("web_chat", "a.first"),
            ("desktop_pet", "pet.only"),
        ] {
            sqlx::query("INSERT INTO product_events(source_app,event_id,schema_version,event_type) VALUES($1,$2,1,$3)")
                .bind(source).bind(uuid::Uuid::new_v4().to_string()).bind(kind)
                .execute(&pool).await.unwrap();
        }
        let rows = super::summary_for_source(&pool, "web_chat").await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (&rows[0].event_type, rows[0].total),
            (&"a.first".to_owned(), 2)
        );
        assert_eq!(
            (&rows[1].event_type, rows[1].total),
            (&"z.last".to_owned(), 1)
        );
        assert!(rows.iter().all(|r| r.source_app == "web_chat"));
        for source in ["not.recorded", "' OR TRUE --"] {
            assert!(
                super::summary_for_source(&pool, source)
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
    }
}
