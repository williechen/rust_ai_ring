use super::sql;

pub async fn configure(pool: &sqlx::SqlitePool, id: &str, choice: &str) -> Result<(), String> {
    if !matches!(choice, "none" | "walk" | "sit") {
        return Err("閒置資格選項不合法".into());
    }
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|_| "無法開始資格交易")?;
    crate::activity::ensure_pair(&mut tx, id).await?;
    sqlx::query(sql::SET_CHOICE)
        .bind(id)
        .bind(choice)
        .execute(&mut *tx)
        .await
        .map_err(|_| "無法儲存閒置資格")?;
    tx.commit().await.map_err(|_| "資格提交失敗".into())
}

pub async fn startup(pool: &sqlx::SqlitePool) -> Result<Vec<(String, String, String)>, String> {
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|_| "無法開始啟動交易")?;
    let ids: Vec<String> = sqlx::query_scalar(sql::LOCAL_IDS)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| "無法讀取寵物")?;
    for id in ids {
        crate::activity::ensure_pair(&mut tx, &id).await?;
    }
    let rows = sqlx::query_as(sql::DEFAULTS)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| "無法讀取啟動資格")?;
    tx.commit().await.map_err(|_| "啟動提交失敗")?;
    Ok(rows)
}
