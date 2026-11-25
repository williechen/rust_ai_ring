pub struct SqlitePetDeletion(pub sqlx::SqlitePool);
pub struct SqliteClosedPetDeletion(pub sqlx::SqlitePool);

impl pet_application::deletion::PetDeletion for SqlitePetDeletion {
    async fn delete(&self, id: &str, name: &str) -> Result<(), String> {
        delete_profile(&self.0, id, name, true).await
    }
}

/// 正式Native caller已在同一生命週期鎖內確認生活窗不存在。
impl pet_application::deletion::PetDeletion for SqliteClosedPetDeletion {
    async fn delete(&self, id: &str, name: &str) -> Result<(), String> {
        delete_profile(&self.0, id, name, false).await
    }
}

async fn delete_profile(
    pool: &sqlx::SqlitePool,
    id: &str,
    name: &str,
    require_inactive: bool,
) -> Result<(), String> {
    // 先取得寫入鎖，避免確認狀態後有另一條連線出籠或改名。
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|_| "無法開始刪除")?;

    let row: Option<(String, bool)> = sqlx::query_as(
        r#"SELECT name
            ,NOT EXISTS(
                SELECT 1 
                FROM pet_activity a 
                WHERE a.pet_id=p.id 
                    AND a.owner_id=p.owner_id 
                    AND a.is_activity=1
            ) 
        FROM pet_profile p 
        WHERE p.id=? 
            AND p.owner_id='local-owner'"#,
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| "無法查詢刪除對象")?;

    let Some((current, caged)) = row else {
        return Err("找不到可刪除的寵物".into());
    };

    if current != name {
        return Err("名稱已變更，請重新載入清單後確認".into());
    }

    if require_inactive && !caged {
        return Err("請先在互動將寵物回籠，再刪除".into());
    }

    for statement in [
        "DELETE FROM pet_extra WHERE pet_id=? AND owner_id='local-owner'",
        "DELETE FROM pet_study WHERE pet_id=? AND owner_id='local-owner'",
        "DELETE FROM pet_life_log WHERE pet_id=? AND owner_id='local-owner'",
        "DELETE FROM pet_activity WHERE pet_id=? AND owner_id='local-owner'",
    ] {
        sqlx::query(statement)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|_| "刪除失敗，資料未變更")?;
    }

    sqlx::query("DELETE FROM pet_profile WHERE id=? AND owner_id='local-owner'")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|_| "刪除失敗，資料未變更")?;
    tx.commit().await.map_err(|_| "刪除提交失敗")?;

    Ok(())
}
