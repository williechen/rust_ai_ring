use pet_application::pet_names::{NamedPet, PetNames};

pub struct SqlitePetNames(pub sqlx::SqlitePool);

impl PetNames for SqlitePetNames {
    async fn create(&self, name: &str) -> Result<NamedPet, String> {
        let id = uuid::Uuid::new_v4().to_string();

        let mut tx = self.0.begin().await.map_err(|_| "無法開始儲存")?;

        sqlx::query(
            r#"INSERT INTO pet_owner(id
                ,name
            ) VALUES('local-owner','本機使用者') ON CONFLICT(id) DO NOTHING"#,
        )
        .execute(&mut *tx)
        .await
        .map_err(|_| "無法儲存主人")?;

        sqlx::query(
            r#"INSERT INTO pet_profile(id
                ,name
                ,species
                ,owner_id
                ,is_default_closure
            ) VALUES(?1,?2,'tortoise','local-owner',1)"#,
        )
        .bind(&id)
        .bind(&name)
        .execute(&mut *tx)
        .await
        .map_err(|_| "無法儲存寵物")?;

        crate::activity::ensure_pair(&mut tx, &id).await?;

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
        .fetch_one(&self.0)
        .await
        .map_err(|_| "已提交但讀回失敗，請先查核資料，勿立即重送")?;

        Ok(NamedPet {
            id: saved_id,
            name: saved_name,
            species,
        })
    }

    async fn active(&self) -> Result<Option<NamedPet>, String> {
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
        .fetch_optional(&self.0)
        .await
        .map_err(|_| "無法讀取生活窗寵物".to_owned())?;

        Ok(row.map(|(id, name, species)| NamedPet { id, name, species }))
    }
    async fn list(&self) -> Result<Vec<NamedPet>, String> {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            r#"SELECT id
                ,name
                ,species 
            FROM pet_profile 
            WHERE owner_id='local-owner' 
            ORDER BY name
                ,id"#,
        )
        .fetch_all(&self.0)
        .await
        .map_err(|_| "無法讀取已儲存寵物")?;
        Ok(rows
            .into_iter()
            .map(|(id, name, species)| NamedPet { id, name, species })
            .collect())
    }
    async fn find(&self, id: &str) -> Result<NamedPet, String> {
        let row: Option<(String, String, String)> = sqlx::query_as(
            r#"SELECT id
                ,name
                ,species 
            FROM pet_profile 
            WHERE id=?1 
                AND owner_id='local-owner'"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .map_err(|_| "無法讀取寵物")?;
        row.map(|(id, name, species)| NamedPet { id, name, species })
            .ok_or_else(|| "找不到寵物".into())
    }

    async fn rename(&self, id: &str, name: &str) -> Result<NamedPet, String> {
        // 同一寫入回傳目標列，避免提交後另一次查詢讀到其他請求的新名稱。
        let row: Option<(String, String, String)> = sqlx::query_as(
            r#"UPDATE pet_profile 
            SET name=?1
                ,updated_at=CURRENT_TIMESTAMP
            WHERE id=?2 
                AND owner_id='local-owner' 
            RETURNING id
                ,name
                ,species"#,
        )
        .bind(name)
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(d) if d.is_unique_violation() => "寵物名稱已存在".to_string(),
            _ => "寵物更名失敗".to_string(),
        })?;
        row.map(|(id, name, species)| NamedPet { id, name, species })
            .ok_or_else(|| "找不到寵物".into())
    }
}
