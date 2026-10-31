use sqlx::SqlitePool;

/// Explicit exclusion takes precedence. This slice leaves existing public-room semantics intact.
pub async fn is_excluded(pool: &SqlitePool, member: &str, room: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM mbr_exclude WHERE mbr_user_id=?1 AND room_id=?2)",
    )
    .bind(member)
    .bind(room)
    .fetch_one(pool)
    .await
}
