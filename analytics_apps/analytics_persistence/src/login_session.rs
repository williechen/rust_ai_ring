use sqlx::PgPool;

pub type SessionRow = (bool, bool, String, String, String);

/// The caller hashes the token; this adapter owns only the PostgreSQL lookup.
pub async fn lookup(pool: &PgPool, token_hash: &str) -> Result<Option<SessionRow>, sqlx::Error> {
    sqlx::query_as(
        r#"SELECT s.restricted
            ,u.is_init_pd
            ,u.user_pd
            ,s.credential_fingerprint
            ,u.id
        FROM uac_login_session s
        JOIN uac_user u
            ON u.id=s.user_id
        WHERE s.token_hash=$1
            AND s.expires_at>floor(extract(epoch from now()))::bigint
            AND u.user_status='active'"#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

/// 密碼變更的交易同時撤銷等待狀態與所有既有工作階段。
pub async fn revoke_after_password_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM uac_login_attempt WHERE user_id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM uac_login_session WHERE user_id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
