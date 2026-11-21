use pet_server::pet_store::PetStore;
#[sqlx::test(migrations = "../migrations")]
async fn confirmed_caged_pet_is_deleted_atomically(pool: sqlx::SqlitePool) {
    let store = PetStore { pool: pool.clone() };
    let a = store.create("甲").await.unwrap();
    let b = store.create("乙").await.unwrap();
    store.select(&a.id).await.unwrap();
    store.set_caged(&a.id, true).await.unwrap();
    for table in ["pet_extra", "pet_study", "pet_life_log"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {table}(id,pet_id,owner_id) VALUES('child',?,'local-owner')"
        )))
        .bind(&a.id)
        .execute(&pool)
        .await
        .unwrap();
    }
    store.delete(&a.id, "甲").await.unwrap();
    assert_eq!(store.list().await.unwrap(), vec![b]);
    for table in [
        "pet_profile",
        "pet_extra",
        "pet_study",
        "pet_life_log",
        "pet_activity",
        "local_pet_selection",
    ] {
        let key = if table == "pet_profile" {
            "id"
        } else {
            "pet_id"
        };
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM {table} WHERE {key}=?"
        )))
        .bind(&a.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(n, 0, "{table}");
    }
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn live_foreign_changed_missing_and_failed_deletes_preserve_data(pool: sqlx::SqlitePool) {
    let store = PetStore { pool: pool.clone() };
    let p = store.create("保留").await.unwrap();
    store.select(&p.id).await.unwrap();
    assert!(store.delete(&p.id, "保留").await.is_err());
    store.set_caged(&p.id, true).await.unwrap();
    assert!(store.delete(&p.id, "舊名").await.is_err());
    assert!(store.delete("missing", "保留").await.is_err());
    sqlx::query("INSERT INTO pet_owner(id,name) VALUES('other','其他主人')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO pet_profile(id,name,species,owner_id) VALUES('foreign','其他寵物','tortoise','other')").execute(&pool).await.unwrap();
    assert!(store.delete("foreign", "其他寵物").await.is_err());
    sqlx::query("CREATE TRIGGER reject_delete BEFORE DELETE ON pet_profile BEGIN SELECT RAISE(ABORT,'fixture'); END").execute(&pool).await.unwrap();
    assert!(store.delete(&p.id, "保留").await.is_err());
    assert_eq!(store.profile(&p.id).await.unwrap().name, "保留");
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pet_activity WHERE pet_id=?")
        .bind(&p.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 2);
}
