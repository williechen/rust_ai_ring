pub mod activity;
pub use activity::set_caged;
pub use activity::{ActivitySnapshot, ActivityStatus};

pub async fn read_caged(pool: &sqlx::SqlitePool, id: &str) -> Result<bool, String> {
    Ok(!activity::status(pool, id).await?.outside)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UncagedPet {
    pub id: String,
    pub name: String,
    pub species: String,
}

pub async fn list_uncaged(pool: &sqlx::SqlitePool) -> Result<Vec<UncagedPet>, String> {
    let rows:Vec<(String,String,String)>=sqlx::query_as("SELECT p.id,p.name,p.species FROM pet_profile p WHERE p.owner_id='local-owner' AND EXISTS(SELECT 1 FROM pet_activity a WHERE a.pet_id=p.id AND a.owner_id=p.owner_id AND a.is_activity=1) ORDER BY p.name,p.id")
        .fetch_all(pool).await.map_err(|_| "無法讀取出籠寵物".to_owned())?;
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
            set_caged(&pool, id, false).await.unwrap();
        }
        set_caged(&pool, "pet-b", true).await.unwrap();
        for (id, expected) in [("pet-a", 0_i64), ("pet-b", 1), ("pet-c", 0)] {
            let flag: i64 =
                 sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM pet_activity a WHERE a.pet_id=pet_profile.id AND a.owner_id=pet_profile.owner_id AND a.is_activity=1) FROM pet_profile WHERE id=?1")
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(flag, expected, "{id}");
        }
        assert!(set_caged(&pool, "missing-id", true).await.is_err());
        set_caged(&pool, "pet-b", false).await.unwrap();
        let flag: i64 =
            sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM pet_activity a WHERE a.pet_id='pet-b' AND a.owner_id='local-owner' AND a.is_activity=1)")
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
        for id in ["pet-a", "pet-c"] {
            super::set_caged(&pool, id, false).await.unwrap();
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

#[cfg(test)]
mod day37_open_state_tests {
    use super::*;
    #[sqlx::test(migrations = "../migrations")]
    async fn read_flag_and_fail_closed_for_missing_pet(pool: sqlx::SqlitePool) {
        sqlx::query("INSERT INTO pet_owner(id,name) VALUES('local-owner','本機')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO pet_profile(id,name,species,owner_id,is_default_closure)
            VALUES('a','A','tortoise','local-owner',1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(read_caged(&pool, "a").await.unwrap());
        set_caged(&pool, "a", false).await.unwrap();
        assert!(!read_caged(&pool, "a").await.unwrap());
        assert!(read_caged(&pool, "missing").await.is_err());
        pool.close().await;
        assert!(read_caged(&pool, "a").await.is_err());
    }
}
