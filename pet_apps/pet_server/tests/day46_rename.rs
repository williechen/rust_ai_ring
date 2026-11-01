use pet_server::pet_store::PetStore;

#[sqlx::test(migrations = "../migrations")]
async fn rename_updates_only_name_and_preserves_activity_and_identity(pool: sqlx::SqlitePool) {
    let store = PetStore { pool: pool.clone() };
    let pet = store.create("原本名字").await.unwrap();
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pet_activity WHERE pet_id=?1")
        .bind(&pet.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, 2);
    let renamed = store.rename(&pet.id, "新名字").await.unwrap();
    assert_eq!(renamed.id, pet.id);
    assert_eq!(renamed.name, "新名字");
    assert_eq!(renamed.species, pet.species);
    assert_eq!(store.profile(&pet.id).await.unwrap().name, "新名字");
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pet_activity WHERE pet_id=?1")
        .bind(&pet.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, before);
    assert!(store.rename("missing-id", "合法名字").await.is_err());
    assert!(store.rename(&pet.id, "   ").await.is_err());
    assert_eq!(store.profile(&pet.id).await.unwrap().name, "新名字");
}
