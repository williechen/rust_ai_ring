use sqlx::{PgPool, Postgres, Transaction};

/// 缺少設定時 fail closed，不無聲套用寫死的預設秒數。
pub async fn read(pool: &PgPool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT idle_seconds FROM analytics_session_policy WHERE id=1")
        .fetch_one(pool)
        .await
}

pub async fn update(tx: &mut Transaction<'_, Postgres>, seconds: i64) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE analytics_session_policy SET idle_seconds=$1,updated_at=now()
         WHERE id=1 RETURNING idle_seconds",
    )
    .bind(seconds)
    .fetch_one(&mut **tx)
    .await
}
