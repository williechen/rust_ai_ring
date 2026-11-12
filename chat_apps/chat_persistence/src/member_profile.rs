use sqlx::SqlitePool;

pub async fn read(pool: &SqlitePool, member_id: &str) -> Result<(String, String), sqlx::Error> {
    sqlx::query_as("SELECT user_name,user_email FROM mbr_user WHERE id=?1")
        .bind(member_id)
        .fetch_one(pool)
        .await
}
