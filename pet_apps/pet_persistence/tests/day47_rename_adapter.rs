use pet_application::pet_names::rename_pet;
use pet_persistence::pet_names::SqlitePetNames;

#[sqlx::test(migrations = "../migrations")]
async fn rename_use_case_preserves_owner_activity_and_other_profiles(pool: sqlx::SqlitePool) {
    sqlx::query(
        "INSERT INTO pet_owner(id,name) VALUES('local-owner','主人'),('foreign','其他主人')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO pet_profile(id,name,species,owner_id,is_default_closure) VALUES('a','第一隻','tortoise','local-owner',1),('b','第二隻','tortoise','foreign',1)").execute(&pool).await.unwrap();
    let store = SqlitePetNames(pool.clone());
    assert_eq!(
        rename_pet(&store, "a", "  新名稱  ").await.unwrap().name,
        "新名稱"
    );
    assert!(rename_pet(&store, "b", "越權").await.is_err());
    assert!(rename_pet(&store, "a", " ").await.is_err());
    let rows: Vec<(String, String, String, i64)> =
        sqlx::query_as("SELECT id,name,owner_id,is_default_closure FROM pet_profile ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        rows,
        vec![
            ("a".into(), "新名稱".into(), "local-owner".into(), 1),
            ("b".into(), "第二隻".into(), "foreign".into(), 1)
        ]
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM pet_activity")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
