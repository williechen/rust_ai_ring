use pet_application::settings::{PetSettings, SettingsProfile};
pub struct SqlitePetSettings(pub sqlx::SqlitePool);
impl PetSettings for SqlitePetSettings {
    async fn load(&self, id: &str) -> Result<SettingsProfile, String> {
        let mut tx = self.0.begin().await.map_err(|_| "無法讀取設定")?;
        let p = read_profile(&mut tx, id).await?;
        tx.commit().await.map_err(|_| "無法完成設定讀取")?;
        Ok(p)
    }

    async fn save_checked(
        &self,
        id: &str,
        p: &SettingsProfile,
        expected: &SettingsProfile,
    ) -> Result<SettingsProfile, String> {
        let mut tx = self
            .0
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|_| "無法取得設定寫入鎖，請稍後重試")?;
        let current = read_profile(&mut tx, id).await?;
        if &current != expected {
            return Err("設定已被其他操作更新；草稿保留，請先重新讀取再修改".into());
        }
        write_profile(&mut tx, id, p).await?;
        tx.commit().await.map_err(|_| "無法提交寵物設定")?;
        Ok(p.clone())
    }
    async fn save(&self, id: &str, p: &SettingsProfile) -> Result<SettingsProfile, String> {
        let mut tx = self.0.begin().await.map_err(|_| "無法開始設定儲存")?;
        write_profile(&mut tx, id, p).await?;
        tx.commit().await.map_err(|_| "無法提交寵物設定")?;
        Ok(p.clone())
    }
}

#[cfg(test)]
mod day64_notification_tests {
    use super::*;
    use pet_application::settings::{ProfileNotification, resynchronize, save_and_notify};
    struct Failed;
    impl ProfileNotification for Failed {
        fn notify(&self, _: &str, _: &SettingsProfile) -> Result<(), String> {
            Err("通知故障".into())
        }
    }
    #[sqlx::test(migrations = "../migrations")]
    async fn committed_profile_survives_notification_failure_and_retry_does_not_write(
        pool: sqlx::SqlitePool,
    ) {
        sqlx::query(
            "INSERT INTO pet_owner(id,name) VALUES('local-owner','本機'),('other-owner','其他')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO pet_profile(id,name,species,owner_id) VALUES('a','原名','tortoise','local-owner'),('b','其他寵物','tortoise','other-owner')").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE fixture_writes(id TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER fixture_update AFTER UPDATE ON pet_profile BEGIN INSERT INTO fixture_writes VALUES(new.id);END").execute(&pool).await.unwrap();
        let store = SqlitePetSettings(pool.clone());
        let mut p = store.load("a").await.unwrap();
        p.name = "已提交".into();
        let value = save_and_notify(&store, &Failed, "a", p.clone())
            .await
            .unwrap();
        assert_eq!(value.profile.name, "已提交");
        assert!(value.sync_warning.is_some());
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM fixture_writes")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            resynchronize(&store, &Failed, "a")
                .await
                .unwrap()
                .profile
                .name,
            "已提交"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM fixture_writes")
                .fetch_one(&pool)
                .await
                .unwrap(),
            count
        );
        assert!(save_and_notify(&store, &Failed, "b", p).await.is_err());
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT name FROM pet_profile WHERE id='b'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            "其他寵物"
        );
    }
}

#[cfg(test)]
mod day65_conflict_tests {
    use super::*;
    #[sqlx::test(migrations = "../migrations")]
    async fn stale_settings_do_not_overwrite_name_or_attributes(pool: sqlx::SqlitePool) {
        sqlx::query("INSERT INTO pet_owner(id,name) VALUES('local-owner','本機')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO pet_profile(id,owner_id,name,species) VALUES('A','local-owner','原名','tortoise')").execute(&pool).await.unwrap();
        let store = SqlitePetSettings(pool);
        let original = store.load("A").await.unwrap();
        let mut external = original.clone();
        external.name = "另一視窗".into();
        external.likes = vec!["陽光".into()];
        store.save("A", &external).await.unwrap();
        let mut draft = original.clone();
        draft.name = "我的草稿".into();
        let result = store.save_checked("A", &draft, &original).await;
        assert!(result.is_err(), "過期草稿不得覆寫");
        assert_eq!(store.load("A").await.unwrap(), external);
        assert_eq!(
            store.save_checked("A", &draft, &external).await.unwrap(),
            draft
        );
        assert!(
            store
                .save_checked("foreign", &draft, &external)
                .await
                .is_err()
        );
    }
}

async fn read_profile(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: &str,
) -> Result<SettingsProfile, String> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT name,species FROM pet_profile WHERE id=?1 AND owner_id='local-owner'",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| "無法讀取寵物設定")?;
    let Some((name, species)) = row else {
        return Err("找不到本機主人的寵物".into());
    };
    let rows:Vec<(String,String)>=sqlx::query_as("SELECT DISTINCT category,content FROM pet_extra WHERE pet_id=?1 AND owner_id='local-owner' AND category IN ('喜好','個性','興趣') AND content IS NOT NULL ORDER BY category,content").bind(id).fetch_all(&mut **tx).await.map_err(|_|"無法讀取寵物屬性")?;
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

    Ok(p)
}

async fn write_profile(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: &str,
    p: &SettingsProfile,
) -> Result<(), String> {
    let changed=sqlx::query("UPDATE pet_profile SET name=?1,species=?2,updated_at=CURRENT_TIMESTAMP WHERE id=?3 AND owner_id='local-owner'").bind(&p.name).bind(&p.species).bind(id).execute(&mut **tx).await.map_err(|_|"無法儲存寵物基本設定")?;
    if changed.rows_affected() != 1 {
        return Err("找不到本機主人的寵物".into());
    }
    sqlx::query("DELETE FROM pet_extra WHERE pet_id=?1 AND owner_id='local-owner' AND category IN ('喜好','個性','興趣')").bind(id).execute(&mut **tx).await.map_err(|_|"無法更新寵物屬性")?;
    for (category, items) in [
        ("喜好", &p.likes),
        ("個性", &p.personality),
        ("興趣", &p.interests),
    ] {
        for value in items {
            sqlx::query("INSERT INTO pet_extra(id,pet_id,owner_id,category,content) VALUES(?1,?2,'local-owner',?3,?4)").bind(uuid::Uuid::new_v4().to_string()).bind(id).bind(category).bind(value).execute(&mut **tx).await.map_err(|_|"無法儲存寵物屬性")?;
        }
    }

    Ok(())
}
