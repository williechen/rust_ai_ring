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

/// 重啟查詢 DTO；不依賴 Tauri 或 server 的 SavedPet。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UncagedPet {
    pub id: String,
    pub name: String,
    pub species: String,
}

/// 只查本機 owner 的未回籠寵物；slot=1 不是出籠清單。
pub async fn list_uncaged(pool: &sqlx::SqlitePool) -> Result<Vec<UncagedPet>, String> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        r#"SELECT id
            , name
            , species
        FROM pet_profile
        WHERE owner_id='local-owner'
            AND COALESCE(is_default_closure, 0)=0
        ORDER BY name
            , id"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| "無法讀取出籠寵物".to_owned())?;
    Ok(rows
        .into_iter()
        .map(|(id, name, species)| UncagedPet { id, name, species })
        .collect())
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

#[cfg(test)]
mod day36_restore_adapter_tests {
    use super::list_uncaged;
    #[sqlx::test(migrations = "../migrations")]
    async fn reads_only_uncaged_local_owner(pool: sqlx::SqlitePool) {
        sqlx::query("INSERT INTO pet_owner(id,name) VALUES('local-owner','本機')")
            .execute(&pool)
            .await
            .unwrap();
        for (id, caged) in [("pet-a", false), ("pet-b", true), ("pet-c", false)] {
            sqlx::query(
                "INSERT INTO pet_profile(id,name,species,owner_id,is_default_closure)
                 VALUES(?1,?2,'tortoise','local-owner',?3)",
            )
            .bind(id)
            .bind(id)
            .bind(caged)
            .execute(&pool)
            .await
            .unwrap();
        }
        let rows = list_uncaged(&pool).await.unwrap();
        assert_eq!(
            rows.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            vec!["pet-a", "pet-c"]
        );
        pool.close().await;
        assert!(list_uncaged(&pool).await.is_err());
    }
}
