use sqlx::SqlitePool;

/// 分類代碼與顯示名稱只由房間分類字典讀取。
pub async fn list(pool: &SqlitePool) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        r#"SELECT code
            ,value 
        FROM codemap 
        WHERE category='room_category' 
        ORDER BY 
            CASE 
                WHEN code='default-category' THEN 0 
            ELSE 1 END
            ,code"#,
    )
    .fetch_all(pool)
    .await
}

pub async fn contains(pool: &SqlitePool, code: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT EXISTS(
            SELECT 1 
            FROM codemap 
            WHERE category='room_category' 
                AND code=?1
        )"#,
    )
    .bind(code)
    .fetch_one(pool)
    .await
}
