use pet_application::settings::{PetSettings, SettingsProfile};
pub struct SqlitePetSettings(pub sqlx::SqlitePool);
impl PetSettings for SqlitePetSettings {
    async fn load(&self, id: &str) -> Result<SettingsProfile, String> {
        let mut tx = self.0.begin().await.map_err(|_| "無法讀取設定")?;
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT name,species FROM pet_profile WHERE id=?1 AND owner_id='local-owner'",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| "無法讀取寵物設定")?;
        let Some((name, species)) = row else {
            return Err("找不到本機主人的寵物".into());
        };
        let rows:Vec<(String,String)>=sqlx::query_as("SELECT DISTINCT category,content FROM pet_extra WHERE pet_id=?1 AND owner_id='local-owner' AND category IN ('喜好','個性','興趣') AND content IS NOT NULL ORDER BY category,content").bind(id).fetch_all(&mut *tx).await.map_err(|_|"無法讀取寵物屬性")?;
        let mut p = SettingsProfile {
            name,
            species,
            likes: vec![],
            personality: vec![],
            interests: vec![],
            appearance: None,
        };
        for (category, value) in rows {
            match category.as_str() {
                "喜好" => p.likes.push(value),
                "個性" => p.personality.push(value),
                "興趣" => p.interests.push(value),
                _ => {}
            }
        }
        tx.commit().await.map_err(|_| "無法完成設定讀取")?;
        Ok(p)
    }

    async fn save(&self, id: &str, p: &SettingsProfile) -> Result<SettingsProfile, String> {
        let mut tx = self.0.begin().await.map_err(|_| "無法開始設定儲存")?;
        let changed=sqlx::query("UPDATE pet_profile SET name=?1,species=?2,updated_at=CURRENT_TIMESTAMP WHERE id=?3 AND owner_id='local-owner'").bind(&p.name).bind(&p.species).bind(id).execute(&mut *tx).await.map_err(|_|"無法儲存寵物基本設定")?;
        if changed.rows_affected() != 1 {
            return Err("找不到本機主人的寵物".into());
        }
        sqlx::query("DELETE FROM pet_extra WHERE pet_id=?1 AND owner_id='local-owner' AND category IN ('喜好','個性','興趣')").bind(id).execute(&mut *tx).await.map_err(|_|"無法更新寵物屬性")?;
        for (category, items) in [
            ("喜好", &p.likes),
            ("個性", &p.personality),
            ("興趣", &p.interests),
        ] {
            for value in items {
                sqlx::query("INSERT INTO pet_extra(id,pet_id,owner_id,category,content) VALUES(?1,?2,'local-owner',?3,?4)").bind(uuid::Uuid::new_v4().to_string()).bind(id).bind(category).bind(value).execute(&mut *tx).await.map_err(|_|"無法儲存寵物屬性")?;
            }
        }
        tx.commit().await.map_err(|_| "無法提交寵物設定")?;
        Ok(p.clone())
    }
}
