use pet_application::pet_names::{NamedPet, PetNames};

pub struct SqlitePetNames(pub sqlx::SqlitePool);

impl PetNames for SqlitePetNames {
    async fn find(&self, id: &str) -> Result<NamedPet, String> {
        let row: Option<(String, String, String)> = sqlx::query_as(
            "SELECT id,name,species FROM pet_profile WHERE id=?1 AND owner_id='local-owner'",
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
            "UPDATE pet_profile SET name=?1,updated_at=CURRENT_TIMESTAMP
             WHERE id=?2 AND owner_id='local-owner' RETURNING id,name,species",
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
