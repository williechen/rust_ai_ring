use pet_persistence::activity::prepare_startup;
use sqlx::SqlitePool;

async fn seed_legacy(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO pet_owner(id,name) VALUES('local-owner','本機'),('foreign','其他主人')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO pet_profile(id,name,species,owner_id,is_default_closure) VALUES
        ('a','舊寵物甲','tortoise','local-owner',1),
        ('b','舊寵物乙','tortoise','local-owner',0),
        ('c','其他寵物','tortoise','foreign',0)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity,is_use_ai)
        VALUES('existing','b','local-owner','sit',1,0),('other','c','foreign','walk',1,0)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO pet_extra(id,pet_id,owner_id,category,content)
        VALUES('p1','a','local-owner','個性','喜'),('p2','a','local-owner','個性','樂')",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../migrations")]
async fn startup_fills_only_missing_local_activity_rows(pool: SqlitePool) {
    seed_legacy(&pool).await;
    for _ in 0..2 {
        let opened = prepare_startup(&pool).await.unwrap();
        assert_eq!(
            opened,
            vec![("b".into(), "舊寵物乙".into(), "tortoise".into())]
        );
        let rows: Vec<(String, String, bool, bool)> = sqlx::query_as(
            "SELECT pet_id,activity_type,is_activity,is_use_ai FROM pet_activity
             ORDER BY pet_id,activity_type",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![
                ("a".into(), "sit".into(), false, false),
                ("a".into(), "walk".into(), false, false),
                ("b".into(), "sit".into(), true, false),
                ("b".into(), "walk".into(), false, false),
                ("c".into(), "walk".into(), true, false),
            ]
        );
    }
    let old: String =
        sqlx::query_scalar("SELECT id FROM pet_activity WHERE pet_id='b' AND activity_type='sit'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(old, "existing");
    let tags: Vec<String> =
        sqlx::query_scalar("SELECT content FROM pet_extra WHERE pet_id='a' ORDER BY content")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(tags, vec!["喜", "樂"]);
}

#[sqlx::test(migrations = "../migrations")]
async fn startup_rolls_back_earlier_rows_if_later_insert_fails(pool: SqlitePool) {
    seed_legacy(&pool).await;
    sqlx::query(
        "CREATE TRIGGER fail_pair BEFORE INSERT ON pet_activity
        WHEN NEW.pet_id='b' BEGIN SELECT RAISE(ABORT,'fixture'); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(prepare_startup(&pool).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM pet_activity")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
}
