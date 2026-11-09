use sqlx::{SqlitePool, migrate::Migrator, sqlite::SqlitePoolOptions};

const PREVIOUS: i64 = 20261108080000;

async fn fixture(upgraded: bool) -> (SqlitePool, Migrator) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let all = sqlx::migrate!("../migrations");
    if upgraded {
        all.run(&pool).await.unwrap();
    } else {
        Migrator::with_migrations(
            all.iter()
                .filter(|m| m.version <= PREVIOUS)
                .cloned()
                .collect(),
        )
        .run(&pool)
        .await
        .unwrap();
    }
    sqlx::raw_sql(
        "INSERT INTO pet_owner(id,name) VALUES ('local-owner','本機'),('other-owner','其他');
         INSERT INTO pet_profile(id,name,species,owner_id,is_default_closure) VALUES
           ('a','甲','tortoise','local-owner',1),('b','乙','tortoise','other-owner',1);",
    )
    .execute(&pool)
    .await
    .unwrap();
    (pool, all)
}

async fn apply(pool: &SqlitePool, sql: &'static str) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(sql).execute(pool).await.map(|_| ())
}

async fn ledger(pool: &SqlitePool) -> Vec<(i64, Vec<u8>)> {
    sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn orphan_rolls_back_and_recovers_without_rewriting_old_data() {
    let (pool, all) = fixture(false).await;
    apply(
        &pool,
        "INSERT INTO codemap(id,category,code,value) VALUES('unrelated','species','roll','其他類別');
         INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity)
           VALUES ('legacy','a','local-owner','roll',1),('blank','b','other-owner',NULL,0);",
    )
    .await
    .unwrap();
    let before = ledger(&pool).await;
    assert!(all.run(&pool).await.is_err(), "未知英文代碼須讓升級回滾");
    assert_eq!(ledger(&pool).await, before);
    apply(
        &pool,
        "INSERT INTO codemap(id,category,code,value) VALUES ('roll','activity','roll','翻身');",
    )
    .await
    .unwrap();
    all.run(&pool).await.unwrap();
    all.run(&pool).await.unwrap();
    let after = ledger(&pool).await;
    assert_eq!(&after[..before.len()], before);
    assert_eq!(after.len(), before.len() + 1);
    let rows: Vec<(String, Option<String>, bool)> =
        sqlx::query_as("SELECT id,activity_type,is_activity FROM pet_activity ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        rows,
        vec![
            ("blank".into(), None, false),
            ("legacy".into(), Some("roll".into()), true)
        ]
    );
}

#[tokio::test]
async fn writes_and_dictionary_changes_cannot_create_orphan_activity() {
    let (pool, _) = fixture(true).await;
    apply(
        &pool,
        "INSERT INTO codemap(id,category,code,value) VALUES
        ('wrong','species','roll','物種'),('play-alias','activity','play','遊戲');",
    )
    .await
    .unwrap();
    assert!(
        apply(
            &pool,
            "INSERT INTO pet_activity(id,pet_id,owner_id,activity_type)
        VALUES ('unknown','a','local-owner','roll');"
        )
        .await
        .is_err(),
        "不同命名空間不可借用"
    );
    apply(
        &pool,
        "INSERT INTO pet_activity(id,pet_id,owner_id,activity_type)
        VALUES ('existing','a','local-owner','play');",
    )
    .await
    .unwrap();
    assert!(
        apply(
            &pool,
            "UPDATE pet_activity SET activity_type='unknown' WHERE id='existing';"
        )
        .await
        .is_err()
    );
    apply(
        &pool,
        "DELETE FROM codemap WHERE id='activity-code:play';
        UPDATE codemap SET value='室內遊戲' WHERE id='play-alias';",
    )
    .await
    .unwrap();
    for sql in [
        "DELETE FROM codemap WHERE id='play-alias';",
        "UPDATE codemap SET category='species' WHERE id='play-alias';",
        "INSERT OR REPLACE INTO codemap(id,category,code,value) VALUES ('play-alias','species','play','替代');",
        "UPDATE OR REPLACE codemap SET id='play-alias' WHERE id='wrong';",
        "DELETE FROM codemap WHERE id='initial-walk';",
    ] {
        assert!(apply(&pool, sql).await.is_err(), "{sql}");
    }
    let code: String =
        sqlx::query_scalar("SELECT activity_type FROM pet_activity WHERE id='existing'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(code, "play");
}

#[tokio::test]
async fn local_activity_status_returns_current_dictionary_labels_and_preserves_owner_scope() {
    let (pool, _) = fixture(true).await;
    apply(&pool, "UPDATE codemap SET value='花園散步' WHERE id='initial-walk';
        INSERT INTO codemap(id,category,code,value) VALUES ('wrong-walk','species','walk','錯誤命名空間');
        INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity)
        VALUES ('remote','b','other-owner','walk',1);").await.unwrap();
    let status = pet_persistence::activity::switch(&pool, "a", true)
        .await
        .unwrap();
    let json = serde_json::to_value(status).unwrap();
    assert_eq!(json["walk_label"], "花園散步");
    assert_eq!(json["sit_label"], "坐下");
    assert_eq!(json["roaming"], true);
    assert!(pet_persistence::activity::status(&pool, "b").await.is_err());
    let active: bool = sqlx::query_scalar("SELECT is_activity FROM pet_activity WHERE id='remote'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(active);
}
