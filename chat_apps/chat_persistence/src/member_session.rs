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
