/// 保留使用者輸入的字面文字，避免百分號、底線及反斜線擴大比對。
pub(crate) fn pattern(input: &str) -> String {
    let text = input.trim();
    if text.is_empty() {
        return String::new();
    }
    format!(
        "%{}%",
        text.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}

#[cfg(all(test, feature = "postgres-integration"))]
mod tests {
    use crate::{daily_report, summary_for_source_and_type};
    async fn event(pool: &sqlx::PgPool, source: &str, kind: &str, time: Option<&str>) {
        sqlx::query("INSERT INTO product_events(source_app,event_id,schema_version,event_type,occurred_at) VALUES($1,$2,1,$3,$4::timestamptz)").bind(source).bind(uuid::Uuid::new_v4().to_string()).bind(kind).bind(time).execute(pool).await.unwrap();
    }
    #[sqlx::test(migrations = "../migrations")]
    async fn contains_casefold_shared_by_summary_daily_and_unknown_with_source_isolation(
        pool: sqlx::PgPool,
    ) {
        event(
            &pool,
            "web_chat",
            "chat.message.sent",
            Some("2026-11-09T10:00:00Z"),
        )
        .await;
        event(
            &pool,
            "web_chat",
            "CHAT.MESSAGE.failed",
            Some("2026-11-09T11:00:00Z"),
        )
        .await;
        event(&pool, "web_chat", "chat.message.sent", None).await;
        event(
            &pool,
            "web_chat",
            "room.created",
            Some("2026-11-09T12:00:00Z"),
        )
        .await;
        event(
            &pool,
            "desktop_pet",
            "chat.message.sent",
            Some("2026-11-09T10:00:00Z"),
        )
        .await;
        sqlx::query("UPDATE codemap SET value='訊息通知' WHERE category='event_type' AND wire_value IN ('chat.message.sent','CHAT.MESSAGE.failed')").execute(&pool).await.unwrap();
        assert_eq!(
            summary_for_source_and_type(&pool, "web_chat", "通知")
                .await
                .unwrap()
                .len(),
            2
        );
        let rows = summary_for_source_and_type(&pool, "web_chat", "  MeSsAgE  ")
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows.iter().map(|r| r.total).sum::<i64>(), 3);
        let daily = daily_report::read(
            &pool,
            "web_chat",
            "MeSsAgE",
            Some("2026-11-09"),
            Some("2026-11-10"),
        )
        .await
        .unwrap();
        assert_eq!(daily.rows, vec![("2026-11-09".into(), 2)]);
        assert_eq!(daily.unknown_dates, 1);
        assert_eq!(
            summary_for_source_and_type(&pool, "desktop_pet", "MESSAGE")
                .await
                .unwrap()[0]
                .total,
            1
        );
    }
    #[sqlx::test(migrations = "../migrations")]
    async fn literal_meta_unicode_and_empty_queries_do_not_expand_or_inject(pool: sqlx::PgPool) {
        for kind in ["種類.睡覺完成", "path.%_\\tail", "path.anything"] {
            event(&pool, "web_chat", kind, Some("2026-11-09T10:00:00Z")).await;
        }
        assert_eq!(
            summary_for_source_and_type(&pool, "web_chat", "睡覺")
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            summary_for_source_and_type(&pool, "web_chat", "%_\\")
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            summary_for_source_and_type(&pool, "web_chat", "  ")
                .await
                .unwrap()
                .len(),
            3
        );
        assert!(
            summary_for_source_and_type(&pool, "web_chat", "' OR TRUE --")
                .await
                .unwrap()
                .is_empty()
        );
        let daily = daily_report::read(
            &pool,
            "web_chat",
            "%_\\",
            Some("2026-11-09"),
            Some("2026-11-10"),
        )
        .await
        .unwrap();
        assert_eq!(daily.rows[0].1, 1);
    }
}
