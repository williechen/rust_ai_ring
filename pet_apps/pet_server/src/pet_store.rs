#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SavedPet {
    pub id: String,
    pub name: String,
    pub species: String,
}

#[derive(Clone)]
pub struct PetStore {
    pub pool: sqlx::SqlitePool,
}

impl PetStore {
    pub async fn profile(&self, id: &str) -> Result<SavedPet, String> {
        let row: Option<(String, String, String)> = sqlx::query_as(
            r#"SELECT id
                ,name
                ,species 
            FROM pet_profile 
            WHERE id=?1 
                AND owner_id='local-owner'"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| "無法讀取寵物")?;

        row.map(|(id, name, species)| SavedPet { id, name, species })
            .ok_or_else(|| "找不到寵物".into())
    }

    pub async fn active(&self) -> Result<Option<SavedPet>, String> {
        let row: Option<(String, String, String)> = sqlx::query_as(
            r#"SELECT p.id
                    ,p.name
                    ,p.species 
                FROM local_pet_selection s 
                JOIN pet_profile p 
                    ON p.id=s.pet_id 
                WHERE s.slot=1 
                    AND p.owner_id='local-owner'
                    AND EXISTS(
                        SELECT 1 
                        FROM pet_activity a 
                        WHERE a.pet_id=p.id 
                            AND a.owner_id=p.owner_id 
                            AND a.is_activity=1
                    )"#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| "無法讀取生活窗寵物".to_owned())?;

        Ok(row.map(|(id, name, species)| SavedPet { id, name, species }))
    }
    pub async fn select(&self, id: &str) -> Result<SavedPet, String> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| "無法開始選取".to_owned())?;

        let row: (String, String, String) = sqlx::query_as(
            r#"SELECT id
                ,name
                ,species 
            FROM pet_profile 
            WHERE id=?1 
                AND owner_id='local-owner'"#,
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| "無法查詢寵物".to_owned())?
        .ok_or_else(|| "找不到可選的寵物".to_owned())?;

        sqlx::query(
            r#"INSERT INTO local_pet_selection(
                    slot
                    ,pet_id
                ) VALUES(1,?1) 
                ON CONFLICT(slot) 
                DO UPDATE 
                    SET pet_id=excluded.pet_id"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|_| "無法儲存選取".to_owned())?;

        pet_persistence::activity::set_caged_in(&mut tx, id, false).await?;

        tx.commit().await.map_err(|_| "選取提交失敗".to_owned())?;
        Ok(SavedPet {
            id: row.0,
            name: row.1,
            species: row.2,
        })
    }

    pub async fn list(&self) -> Result<Vec<SavedPet>, String> {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            r#"SELECT id
                ,name
                ,species 
            FROM pet_profile 
            WHERE owner_id='local-owner' 
            ORDER BY name,id"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|_| "無法讀取已儲存寵物".to_owned())?;

        Ok(rows
            .into_iter()
            .map(|(id, name, species)| SavedPet { id, name, species })
            .collect())
    }

    // apps/desktop_pet/src/pet_store.rs
    // 只將既有 create() 簽名與尾端的返回值更換，
    // INSERT/transaction/提交後 SELECT id,name/錯誤分支完整保留。
    pub async fn create(&self, input: &str) -> Result<SavedPet, String> {
        let name = pet_application::normalize_new_pet_name(input)?;

        let id = uuid::Uuid::new_v4().to_string();

        let mut tx = self.pool.begin().await.map_err(|_| "無法開始儲存")?;

        sqlx::query(
        r#"INSERT INTO pet_owner(id,name) VALUES('local-owner','本機使用者') ON CONFLICT(id) DO NOTHING"#
        ).execute(&mut *tx).await.map_err(|_| "無法儲存主人")?;

        sqlx::query(
         r#"INSERT INTO pet_profile(id,name,species,owner_id,is_default_closure) VALUES(?1,?2,'tortoise','local-owner',1)"#
        ).bind(&id).bind(&name)
        .execute(&mut *tx).await.map_err(|_| "無法儲存寵物")?;

        pet_persistence::activity::ensure_pair(&mut tx, &id).await?;

        tx.commit().await.map_err(|_| "寵物提交失敗")?;

        let (saved_id, saved_name, species): (String, String, String) = sqlx::query_as(
            r#"SELECT id
                ,name
                ,species 
            FROM pet_profile 
            WHERE id=?1 
                AND owner_id='local-owner'"#,
        )
        .bind(&id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| "已提交但讀回失敗，請先查核資料，勿立即重送")?;

        Ok(SavedPet {
            id: saved_id,
            name: saved_name,
            species,
        })
    }

    pub async fn uncaged(&self) -> Result<Vec<SavedPet>, String> {
        let rows = pet_persistence::list_uncaged(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| SavedPet {
                id: row.id,
                name: row.name,
                species: row.species,
            })
            .collect())
    }

    pub async fn set_caged(&self, id: &str, caged: bool) -> Result<(), String> {
        pet_persistence::set_caged(&self.pool, id, caged).await
    }

    pub async fn read_caged(&self, id: &str) -> Result<bool, String> {
        pet_persistence::read_caged(&self.pool, id).await
    }

    pub async fn activity_status(
        &self,
        id: &str,
    ) -> Result<pet_persistence::ActivityStatus, String> {
        pet_persistence::activity::status(&self.pool, id).await
    }

    pub async fn snapshot(&self, id: &str) -> Result<pet_persistence::ActivitySnapshot, String> {
        pet_persistence::activity::snapshot(&self.pool, id).await
    }

    pub async fn restore(
        &self,
        id: &str,
        s: &pet_persistence::ActivitySnapshot,
    ) -> Result<(), String> {
        pet_persistence::activity::restore(&self.pool, id, s).await
    }

    pub async fn switch_activity(
        &self,
        id: &str,
        walking: bool,
    ) -> Result<pet_persistence::ActivityStatus, String> {
        pet_persistence::activity::switch(&self.pool, id, walking).await
    }

    pub async fn set_default_caged(
        &self,
        id: &str,
        caged: bool,
    ) -> Result<pet_persistence::ActivityStatus, String> {
        pet_persistence::activity::set_default_caged(&self.pool, id, caged).await
    }

    pub async fn prepare_startup(&self) -> Result<Vec<SavedPet>, String> {
        pet_persistence::activity::prepare_startup(&self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|(id, name, species)| SavedPet { id, name, species })
                    .collect()
            })
    }

    pub async fn default_pets(&self) -> Result<Vec<SavedPet>, String> {
        sqlx::query_as::<_, (String, String, String)>(
            r#"SELECT id
                ,name
                ,species 
            FROM pet_profile 
            WHERE owner_id='local-owner' 
                 AND COALESCE(is_default_closure,0)=0 
            ORDER BY name
                ,id"#,
        )
        .fetch_all(&self.pool)
        .await
        .map(|r| {
            r.into_iter()
                .map(|(id, name, species)| SavedPet { id, name, species })
                .collect()
        })
        .map_err(|_| "無法讀取啟動預設寵物".into())
    }
}

#[cfg(test)]
mod day22_tests {
    use super::*;
    #[sqlx::test(migrations = "../migrations")]
    async fn day22_create_pet_persists(pool: sqlx::SqlitePool) {
        let store = PetStore { pool: pool.clone() };
        assert_eq!(store.create(" 小橘 ").await.unwrap().name, "小橘");

        let row: (String, String, String) =
            sqlx::query_as(r#"SELECT name,species,owner_id FROM pet_profile"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            row,
            ("小橘".into(), "tortoise".into(), "local-owner".into())
        );
        assert!(store.create("  ").await.is_err());

        let (count,): (i64,) = sqlx::query_as(r#"SELECT count(*) FROM pet_profile"#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);

        let (violations,): (i64,) =
            sqlx::query_as(r#"SELECT count(*) FROM pragma_foreign_key_check"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(violations, 0);
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn day28_caged_pet_is_not_active(pool: sqlx::SqlitePool) {
        let store = PetStore { pool };
        let pet = store.create("阿龜").await.unwrap();
        store.select(&pet.id).await.unwrap();
        store.set_caged(&pet.id, true).await.unwrap();
        assert_eq!(store.active().await.unwrap(), None);
        store.select(&pet.id).await.unwrap();
        assert_eq!(store.active().await.unwrap(), Some(pet));
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn day32_selecting_other_pet_changes_only_active_slot(pool: sqlx::SqlitePool) {
        let store = PetStore { pool: pool.clone() };
        let orange = store.create("小橘").await.unwrap();
        let turtle = store.create("阿龜").await.unwrap();
        store.select(&orange.id).await.unwrap();
        assert_eq!(store.active().await.unwrap(), Some(orange.clone()));
        store.select(&turtle.id).await.unwrap();
        assert_eq!(store.active().await.unwrap(), Some(turtle.clone()));
        assert_eq!(store.list().await.unwrap().len(), 2);
        let selected: String =
            sqlx::query_scalar("SELECT pet_id FROM local_pet_selection WHERE slot=1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(selected, turtle.id);
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn day36_restart_restores_all_uncaged_not_just_selected(pool: sqlx::SqlitePool) {
        let store = PetStore { pool: pool.clone() };
        let first = store.create("小橘").await.unwrap();
        let caged = store.create("阿龜").await.unwrap();
        let third = store.create("小白").await.unwrap();
        store.select(&first.id).await.unwrap();
        store.select(&third.id).await.unwrap();
        store.set_caged(&caged.id, true).await.unwrap();
        assert_eq!(store.active().await.unwrap(), Some(third.clone()));

        // 模擬程式重啟：以同一持久化 DB 重新建立 Store，不依賴 Runtime 記憶體。
        let restarted = PetStore { pool: pool.clone() };
        let pets = restarted.uncaged().await.unwrap();
        let ids: std::collections::HashSet<_> = pets.into_iter().map(|p| p.id).collect();
        assert_eq!(ids, [first.id, third.id].into_iter().collect());
        assert!(!ids.contains(&caged.id));
    }
}

#[cfg(test)]
mod day34_create_name_tests {
    use super::PetStore;

    #[sqlx::test(migrations = "../migrations")]
    async fn day34_bad_name_never_writes_owner_or_profile(pool: sqlx::SqlitePool) {
        let store = PetStore { pool: pool.clone() };
        let before: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM pet_owner), (SELECT count(*) FROM pet_profile)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        for name in ["小\0橘", "小\n橘", "小\t橘", "\u{7f}"] {
            assert!(store.create(name).await.is_err());
        }
        let after: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM pet_owner), (SELECT count(*) FROM pet_profile)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(after, before);
        let created = store.create("  小橘  ").await.unwrap();
        assert_eq!(created.name, "小橘");
        assert_eq!(store.profile(&created.id).await.unwrap(), created);
    }
}

#[cfg(test)]
mod day35_cage_caller_tests {
    use super::PetStore;

    #[sqlx::test(migrations = "../migrations")]
    async fn live_store_cages_only_one_saved_pet(pool: sqlx::SqlitePool) {
        let store = PetStore { pool: pool.clone() };
        let first = store.create("小橘").await.unwrap();
        let second = store.create("阿龜").await.unwrap();
        store.set_caged(&first.id, false).await.unwrap();
        store.set_caged(&second.id, true).await.unwrap();
        let first_flag: i64 =
            sqlx::query_scalar(r#"SELECT NOT EXISTS(SELECT 1 FROM pet_activity a WHERE a.pet_id=pet_profile.id AND a.owner_id=pet_profile.owner_id AND a.is_activity=1) FROM pet_profile WHERE id=?1"#)
                .bind(&first.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let second_flag: i64 =
            sqlx::query_scalar(r#"SELECT NOT EXISTS(SELECT 1 FROM pet_activity a WHERE a.pet_id=pet_profile.id AND a.owner_id=pet_profile.owner_id AND a.is_activity=1) FROM pet_profile WHERE id=?1"#)
                .bind(&second.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((first_flag, second_flag), (0, 1));
        assert!(store.set_caged("not-a-pet", true).await.is_err());
    }
}
