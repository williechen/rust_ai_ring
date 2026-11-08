use sqlx::SqlitePool;

pub async fn save_authorized(
    pool: &SqlitePool,
    token_hash: &str,
    member_id: &str,
    room_id: &str,
    message_id: &str,
    content: &str,
) -> Result<bool, sqlx::Error> {
    // 先取得寫鎖，讓 SQLite 在鎖等待結束後才讀取現在時間及授權資料。
    let mut write = pool.begin_with("BEGIN IMMEDIATE").await?;
    let result = sqlx::query(
        "INSERT INTO messages(id,room_id,author_kind,author_id,content)
         SELECT ?1,?2,'user',?3,?4
         WHERE EXISTS (
             SELECT 1 FROM mbr_session s JOIN mbr_user u ON u.id=s.mbr_user_id
             WHERE s.token_hash=?5 AND s.mbr_user_id=?3
               AND s.expires_at>unixepoch() AND u.user_status='active'
               AND u.is_init_pd=FALSE
         )
         AND EXISTS (SELECT 1 FROM rooms WHERE id=?2)
         AND NOT EXISTS (
             SELECT 1 FROM mbr_exclude WHERE mbr_user_id=?3 AND room_id=?2
         )",
    )
    .bind(message_id)
    .bind(room_id)
    .bind(member_id)
    .bind(content)
    .bind(token_hash)
    .execute(&mut *write)
    .await?;
    write.commit().await?;
    Ok(result.rows_affected() == 1)
}
