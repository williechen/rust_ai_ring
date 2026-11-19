use analytics_application::daily_report::{DailyReport, date_shape};
#[derive(Debug)]
pub enum DailyError {
    Invalid,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for DailyError {
    fn from(e: sqlx::Error) -> Self {
        if e.as_database_error()
            .and_then(|e| e.code())
            .is_some_and(|c| matches!(c.as_ref(), "22007" | "22008" | "22009"))
        {
            Self::Invalid
        } else {
            Self::Database(e)
        }
    }
}

/// 各日與未知日期數在同一唯讀快照讀取；只使用明確發生時間。
pub async fn read(
    pool: &sqlx::PgPool,
    source: &str,
    kind: &str,
    start: Option<&str>,
    end: Option<&str>,
) -> Result<DailyReport, DailyError> {
    if start.is_some_and(|s| !date_shape(s)) || end.is_some_and(|s| !date_shape(s)) {
        return Err(DailyError::Invalid);
    }

    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;

    let (start, end, span): (String, String, i32) = sqlx::query_as(
        r#"WITH r AS (
            SELECT COALESCE($1::date,(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')::date-6) a
                ,COALESCE($2::date,(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')::date+1) b
        )
        SELECT a::text
            ,b::text
            ,b-a
        FROM r"#,
    )
    .bind(start)
    .bind(end)
    .fetch_one(&mut *tx)
    .await?;

    if !(0..=31).contains(&span) {
        return Err(DailyError::Invalid);
    }

    let rows:Vec<(String,i64)> = sqlx::query_as(
        r#"WITH days AS (
            SELECT generate_series($3::date::timestamp,($4::date-1)::timestamp,interval '1 day')::date AS report_day
        )
        SELECT report_day::text,COUNT(p.event_id)::bigint
        FROM days
        LEFT JOIN product_events p
            ON p.source_app=$1
            AND p.occurred_at >= (report_day::timestamp AT TIME ZONE 'UTC')
            AND p.occurred_at < ((report_day+1)::timestamp AT TIME ZONE 'UTC')
            AND ($2='' OR EXISTS(
                SELECT 1
                FROM codemap c
                WHERE c.category='event_type'
                    AND c.code=p.event_type
                    AND c.wire_value=$2
                )
            )
        GROUP BY report_day
        ORDER BY report_day"#
    )
    .bind(source)
    .bind(kind)
    .bind(&start)
    .bind(&end)
    .fetch_all(&mut *tx)
    .await?;

    let unknown_dates: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)::bigint 
        FROM product_events p 
        WHERE p.source_app=$1 
            AND p.occurred_at IS NULL 
            AND ($2='' OR EXISTS(
                SELECT 1 
                FROM codemap c 
                WHERE c.category='event_type' 
                    AND c.code=p.event_type 
                    AND c.wire_value=$2
                )
            )"#,
    )
    .bind(source)
    .bind(kind)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(DailyReport {
        start,
        end,
        rows,
        unknown_dates,
    })
}

pub struct PgDailyReport(pub sqlx::PgPool);

impl analytics_application::daily_report::DailyStore for PgDailyReport {
    type Error = DailyError;

    async fn read(
        &self,
        source: &str,
        kind: &str,
        start: Option<&str>,
        end: Option<&str>,
    ) -> Result<DailyReport, DailyError> {
        read(&self.0, source, kind, start, end).await
    }
}

#[cfg(all(test, feature = "postgres-integration"))]
mod tests {
    use super::*;
    #[sqlx::test(migrations = "../migrations")]
    async fn utc_boundaries_zero_days_source_filter_and_unknown_dates_are_distinct(
        pool: sqlx::PgPool,
    ) {
        for (source, kind, time) in [
            (
                "web_chat",
                "message.persisted",
                Some("2026-11-08T23:59:59Z"),
            ),
            (
                "web_chat",
                "message.persisted",
                Some("2026-11-09T00:00:00Z"),
            ),
            ("web_chat", "other.event", Some("2026-11-09T10:00:00Z")),
            ("web_chat", "message.persisted", None),
            (
                "desktop_pet",
                "message.persisted",
                Some("2026-11-08T11:00:00Z"),
            ),
        ] {
            sqlx::query("INSERT INTO product_events(source_app,event_id,schema_version,event_type,occurred_at) VALUES($1,$2,1,$3,$4::timestamptz)").bind(source).bind(uuid::Uuid::new_v4().to_string()).bind(kind).bind(time).execute(&pool).await.unwrap();
        }
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO product_events(source_app,event_id,schema_version,event_type,occurred_at) VALUES('web_chat',$1,1,'message.persisted','2026-11-08T12:00:00Z')").bind(uuid::Uuid::new_v4().to_string()).execute(&mut *tx).await.unwrap();
        tx.rollback().await.unwrap();
        let report = read(
            &pool,
            "web_chat",
            "message.persisted",
            Some("2026-11-08"),
            Some("2026-11-11"),
        )
        .await
        .unwrap();
        assert_eq!(
            report.rows,
            vec![
                ("2026-11-08".into(), 1),
                ("2026-11-09".into(), 1),
                ("2026-11-10".into(), 0)
            ]
        );
        assert_eq!(report.unknown_dates, 1);
        assert_eq!(
            read(
                &pool,
                "desktop_pet",
                "message.persisted",
                Some("2026-11-08"),
                Some("2026-11-09")
            )
            .await
            .unwrap()
            .rows[0]
                .1,
            1
        );
        assert_eq!(
            read(
                &pool,
                "web_chat",
                "' OR TRUE --",
                Some("2026-11-08"),
                Some("2026-11-09")
            )
            .await
            .unwrap()
            .rows[0]
                .1,
            0
        );
        for (a, b) in [
            ("2026-02-30", "2026-03-02"),
            ("2026-11-09", "2026-11-08"),
            ("2026-01-01", "2026-03-01"),
            ("now", "2026-11-09"),
        ] {
            assert!(matches!(
                read(&pool, "web_chat", "", Some(a), Some(b)).await,
                Err(DailyError::Invalid)
            ));
        }
        pool.close().await;
        assert!(matches!(
            read(&pool, "web_chat", "", None, None).await,
            Err(DailyError::Database(_))
        ));
    }
}
