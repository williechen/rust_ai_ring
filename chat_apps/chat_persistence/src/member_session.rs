use sqlx::SqlitePool;

/// SQLite adapter accepts a digest, never a raw bearer token or HTTP headers.
pub async fn active_member_id(
    pool: &SqlitePool,
    digest: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT u.id FROM mbr_session s JOIN mbr_user u ON u.id=s.mbr_user_id
         WHERE s.token_hash=?1 AND s.expires_at>unixepoch() AND u.user_status='active'",
    )
    .bind(digest)
    .fetch_optional(pool)
    .await
}

/// 僅有效的會員工作階段可以延長期限。
pub async fn renew(pool: &SqlitePool, digest: &str) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        "UPDATE mbr_session
         SET expires_at=MAX(expires_at,unixepoch()+?2)
         WHERE token_hash=?1 AND expires_at>unixepoch()
         AND EXISTS(SELECT 1 FROM mbr_user u
                    WHERE u.id=mbr_session.mbr_user_id AND u.user_status='active')",
    )
    .bind(digest)
    .bind(chat_application::MEMBER_SESSION_IDLE_SECONDS)
    .execute(pool)
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// 撤銷指定摘要；呼叫者可使用獨立連線或既有交易。
pub async fn revoke<'e, E>(executor: E, digest: &str) -> Result<bool, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let result = sqlx::query("DELETE FROM mbr_session WHERE token_hash=?1")
        .bind(digest)
        .execute(executor)
        .await?;
    Ok(result.rows_affected() == 1)
}
