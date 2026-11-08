use sqlx::{SqlitePool, migrate::Migrator, sqlite::SqlitePoolOptions};

fn through(all: &Migrator, last_version: i64) -> Migrator {
    Migrator::with_migrations(
        all.iter()
            .filter(|m| m.version <= last_version)
            .cloned()
            .collect(),
    )
}

async fn old_database() -> (SqlitePool, Migrator) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let all = sqlx::migrate!("../migrations");
    through(&all, 20261018114025).run(&pool).await.unwrap();
    sqlx::query("INSERT INTO pet_owner(id,name) VALUES('local-owner','Owner')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO pet_profile(id,name,species,owner_id,is_default_closure) VALUES('a','A','tortoise','local-owner',0)").execute(&pool).await.unwrap();
    (pool, all)
}

#[tokio::test]
async fn old_display_names_migrate_to_codes_with_sit_priority_and_preserve_dictionary() {
    let (pool, all) = old_database().await;
    sqlx::query("INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity) VALUES('w','a','local-owner','散步',1),('s','a','local-owner','坐下',1),('p','a','local-owner','play',1)").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO codemap(id,category,code,value) VALUES('custom-walk','activity','walk','自訂散步')").execute(&pool).await.unwrap();
    all.run(&pool)
        .await
        .expect("new migration must run on SQLite");
    all.run(&pool).await.unwrap();
    let activities: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT id,activity_type,is_activity FROM pet_activity WHERE id IN ('w','s','p') ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        activities,
        vec![
            ("p".into(), "play".into(), 1),
            ("s".into(), "sit".into(), 1),
            ("w".into(), "walk".into(), 0)
        ]
    );
    let custom: String = sqlx::query_scalar("SELECT value FROM codemap WHERE id='custom-walk'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(custom, "自訂散步");
    let dictionary_count: i64 = sqlx::query_scalar("SELECT count(*) FROM codemap")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(dictionary_count, 5);
    let activity_count: i64 = sqlx::query_scalar("SELECT count(*) FROM pet_activity")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(activity_count, 3);
    let timestamp: Option<String> =
        sqlx::query_scalar("SELECT created_at FROM codemap WHERE code='sit'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(timestamp.is_some_and(|s| !s.is_empty()));
    let status = pet_persistence::activity::status(&pool, "a").await.unwrap();
    assert!(status.outside && !status.roaming && !status.default_caged);
    pool.close().await;
}

#[tokio::test]
async fn dictionary_id_collision_rolls_back_every_activity_change() {
    let (pool, all) = old_database().await;
    sqlx::query("INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity) VALUES('w','a','local-owner','散步',1)").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO codemap(id,category,code,value) VALUES('initial-sit','other','keep','untouched')").execute(&pool).await.unwrap();
    assert!(all.run(&pool).await.is_err());
    let activities: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT id,activity_type,is_activity FROM pet_activity")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(activities, vec![("w".into(), "散步".into(), 1)]);
    let dictionary: Vec<(String, String)> = sqlx::query_as("SELECT code,value FROM codemap")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(dictionary, vec![("keep".into(), "untouched".into())]);
    let versions: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success=1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(versions, 3);
    pool.close().await;
}

#[tokio::test]
async fn already_upgraded_database_merges_generated_duplicates_and_reopens() {
    use sqlx::sqlite::SqliteConnectOptions;

    let path = std::env::temp_dir().join(format!("pet-day55-{}.sqlite", uuid::Uuid::new_v4()));
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .unwrap();
    let all = sqlx::migrate!("../migrations");
    through(&all, 20261018114025).run(&pool).await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO pet_owner(id,name) VALUES ('local-owner','本機'),('other-owner','其他主人');
         INSERT INTO pet_profile(id,name,species,owner_id,is_default_closure) VALUES
         ('a','甲','tortoise','local-owner',1),('b','乙','tortoise','local-owner',1);
         INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity,is_use_ai,created_at,updated_at) VALUES
         ('old-walk','a','local-owner','散步',1,0,'2020-01-01','2020-01-02'),
         ('old-sit','a','local-owner','坐下',0,0,'2020-02-01','2020-02-02'),
         ('old-play','a','local-owner','玩耍',1,0,'2020-03-01','2020-03-02'),
          ('old-train','a','local-owner','訓練',0,1,'2020-03-03','2020-03-04'),
         ('other-walk','a','other-owner','walk',1,0,'2020-04-01','2020-04-02');
         INSERT INTO pet_extra(id,pet_id,owner_id,category,content) VALUES
         ('mood-1','a','local-owner','personality','喜'),
         ('mood-2','a','local-owner','personality','樂');",
    )
    .execute(&pool)
    .await
    .unwrap();
    through(&all, 20261030140000).run(&pool).await.unwrap();
    sqlx::query("UPDATE pet_activity SET is_activity=1,is_use_ai=1 WHERE id='initial-sit:a'")
        .execute(&pool)
        .await
        .unwrap();
    let prior: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(prior.len(), 5);
    let before = activities(&pool).await;
    assert_eq!(before.len(), 6);
    sqlx::query(
        "CREATE TRIGGER reject_duplicate_delete BEFORE DELETE ON pet_activity
         WHEN OLD.id='initial-sit:a' BEGIN SELECT RAISE(ABORT,'測試刪除失敗'); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    let err = all.run(&pool).await.unwrap_err().to_string();
    assert!(err.contains("測試刪除失敗"), "{err}");
    assert_eq!(activities(&pool).await, before);
    let still_applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(still_applied, 5);
    sqlx::query("DROP TRIGGER reject_duplicate_delete")
        .execute(&pool)
        .await
        .unwrap();

    all.run(&pool).await.unwrap();
    pool.close().await;
    let reopened = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    all.run(&reopened).await.unwrap();

    let rows = activities(&reopened).await;
    eprintln!("升級後本機主人活動：{rows:?}");
    assert_eq!(
        rows,
        vec![
            (
                "old-play".into(),
                "play".into(),
                1,
                0,
                "2020-03-01".into(),
                "2020-03-02".into()
            ),
            (
                "old-sit".into(),
                "sit".into(),
                1,
                1,
                "2020-02-01".into(),
                "2020-02-02".into()
            ),
            (
                "old-walk".into(),
                "walk".into(),
                0,
                0,
                "2020-01-01".into(),
                "2020-01-02".into()
            ),
            (
                "old-train".into(),
                "train".into(),
                0,
                1,
                "2020-03-03".into(),
                "2020-03-04".into()
            ),
        ]
    );
    let current: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&reopened)
            .await
            .unwrap();
    assert_eq!(&current[..prior.len()], prior);
    assert_eq!(current.len(), 7);
    let labels: Vec<(String, String)> = sqlx::query_as(
        "SELECT code,value FROM codemap WHERE category='activity' ORDER BY code,value",
    )
    .fetch_all(&reopened)
    .await
    .unwrap();
    assert_eq!(
        labels,
        vec![
            ("play".into(), "玩耍".into()),
            ("sit".into(), "坐下".into()),
            ("train".into(), "訓練".into()),
            ("walk".into(), "散步".into()),
        ]
    );
    eprintln!("活動字典：{labels:?}");
    let unmerged: Vec<(String, i64)> = sqlx::query_as(
        "SELECT id,is_activity FROM pet_activity
         WHERE owner_id='other-owner' OR pet_id='b' ORDER BY id",
    )
    .fetch_all(&reopened)
    .await
    .unwrap();
    assert_eq!(
        unmerged,
        vec![
            ("initial-sit:b".into(), 0),
            ("initial-walk:b".into(), 0),
            ("other-walk".into(), 1)
        ]
    );
    let extra: Vec<(String, String)> =
        sqlx::query_as("SELECT category,content FROM pet_extra ORDER BY id")
            .fetch_all(&reopened)
            .await
            .unwrap();
    assert_eq!(
        extra,
        vec![
            ("personality".into(), "喜".into()),
            ("personality".into(), "樂".into())
        ]
    );
    let status = pet_persistence::activity::status(&reopened, "a")
        .await
        .unwrap();
    assert!(status.outside && !status.roaming && status.default_caged);
    let walking = pet_persistence::activity::switch(&reopened, "a", true)
        .await
        .unwrap();
    assert!(walking.outside && walking.roaming);
    assert_eq!(activities(&reopened).await.len(), 4);
    reopened.close().await;
    std::fs::remove_file(path).unwrap();
}

async fn activities(pool: &SqlitePool) -> Vec<(String, String, i64, i64, String, String)> {
    sqlx::query_as(
        "SELECT id,activity_type,is_activity,is_use_ai,created_at,updated_at FROM pet_activity
         WHERE pet_id='a' AND owner_id='local-owner' ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn dictionary_conversion_is_atomic_and_rejects_future_display_names() {
    let (pool, all) = old_database().await;
    through(&all, 20261108070000).run(&pool).await.unwrap();
    let before: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(before.len(), 6);
    sqlx::raw_sql(
        "INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity,is_use_ai) VALUES
         ('legacy-custom','a','local-owner','翻滾',1,0),
         ('legacy-alias','a','local-owner','散心',0,0),
         ('legacy-null','a','local-owner',NULL,0,0);
         UPDATE pet_activity SET is_activity=1,is_use_ai=1 WHERE id='initial-walk:a';
         DELETE FROM codemap WHERE id='initial-walk';
         INSERT INTO codemap(id,category,code,value) VALUES
         ('custom-1','activity','roll','翻滾'),
         ('custom-2','activity','tumble','翻滾');",
    )
    .execute(&pool)
    .await
    .unwrap();
    for correction in [
        "DELETE FROM codemap WHERE id='custom-2'",
        "INSERT INTO codemap(id,category,code,value) VALUES ('alias','activity','walk','散心')",
    ] {
        let failure = all.run(&pool).await.unwrap_err().to_string();
        assert!(
            failure.contains("activity_requires_unique_codemap_code"),
            "{failure}"
        );
        let current: Vec<(i64, Vec<u8>)> =
            sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(current, before);
        let stored: String =
            sqlx::query_scalar("SELECT activity_type FROM pet_activity WHERE id='legacy-custom'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored, "翻滾");
        let added: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM codemap WHERE id LIKE 'activity-code:%'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(added, 0);
        sqlx::query(correction).execute(&pool).await.unwrap();
    }

    all.run(&pool).await.unwrap();
    all.run(&pool).await.unwrap();
    let after: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(&after[..6], before);
    assert_eq!(after.len(), 7);
    let restored: Vec<String> = sqlx::query_scalar(
        "SELECT value FROM codemap WHERE category='activity' AND code='walk' ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(restored, ["散步", "散心"]);
    let rows: Vec<(String, Option<String>, i64, i64)> = sqlx::query_as(
        "SELECT id,activity_type,is_activity,is_use_ai FROM pet_activity ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            ("initial-sit:a".into(), Some("sit".into()), 0, 0),
            ("legacy-alias".into(), Some("walk".into()), 1, 1),
            ("legacy-custom".into(), Some("roll".into()), 1, 0),
            ("legacy-null".into(), None, 0, 0),
        ]
    );
    for bad in ["玩耍", "", "walk around", "walk\0bad"] {
        let inserted = sqlx::query(
            "INSERT INTO pet_activity(id,pet_id,owner_id,activity_type) VALUES('invalid','a','local-owner',?1)",
        )
        .bind(bad)
        .execute(&pool)
        .await;
        assert!(inserted.is_err(), "{bad:?}");
        let updated =
            sqlx::query("UPDATE pet_activity SET activity_type=?1 WHERE id='legacy-custom'")
                .bind(bad)
                .execute(&pool)
                .await;
        assert!(updated.is_err(), "{bad:?}");
    }
    assert!(
        pet_persistence::activity::switch(&pool, "a", true)
            .await
            .unwrap()
            .roaming
    );
    eprintln!("原六筆 checksum 保留，歧義或缺少代碼均回滾，中文 INSERT／UPDATE 均拒絕");
}
