/// 只負責逐寵物回籠標記；Tauri 視窗／Runtime 不屬於資料層。
pub async fn set_caged(pool: &sqlx::SqlitePool, id: &str, caged: bool) -> Result<(), String> {
    let result = sqlx::query(
        "UPDATE pet_profile SET is_default_closure=?1,
         updated_at=CURRENT_TIMESTAMP
         WHERE id=?2 AND owner_id='local-owner'",
    )
    .bind(caged)
    .bind(id)
    .execute(pool)
    .await
    .map_err(|_| "無法保存回籠狀態".to_owned())?;
    if result.rows_affected() != 1 {
        return Err("找不到可回籠的寵物".into());
    }
    Ok(())
}

#[cfg(test)]
mod day35_cage_adapter_tests {
    use super::set_caged;

    #[sqlx::test(migrations = "../migrations")]
    async fn only_target_pet_changes_and_missing_id_fails(pool: sqlx::SqlitePool) {
        sqlx::query("INSERT INTO pet_owner(id,name) VALUES('local-owner','本機主人')")
            .execute(&pool)
            .await
            .unwrap();
        for id in ["pet-a", "pet-b", "pet-c"] {
            sqlx::query("INSERT INTO pet_profile(id,name,species,owner_id) VALUES(?1,?2,'tortoise','local-owner')")
                .bind(id).bind(id).execute(&pool).await.unwrap();
        }
        set_caged(&pool, "pet-b", true).await.unwrap();
        for (id, expected) in [("pet-a", 0_i64), ("pet-b", 1), ("pet-c", 0)] {
            let flag: i64 =
                sqlx::query_scalar("SELECT is_default_closure FROM pet_profile WHERE id=?1")
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(flag, expected, "{id}");
        }
        assert!(set_caged(&pool, "missing-id", true).await.is_err());
        set_caged(&pool, "pet-b", false).await.unwrap();
        let flag: i64 =
            sqlx::query_scalar("SELECT is_default_closure FROM pet_profile WHERE id='pet-b'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(flag, 0);
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn failed_database_update_is_not_success(pool: sqlx::SqlitePool) {
        pool.close().await;
        assert!(set_caged(&pool, "pet-a", true).await.is_err());
    }
}
