use pet_application::settings::{self, SettingsProfile};
use pet_persistence::settings::SqlitePetSettings;

fn profile() -> SettingsProfile {
    SettingsProfile {
        name: " 新名字 ".into(),
        species: "tortoise".into(),
        likes: vec!["草".into(), " 太陽 ".into(), "草".into()],
        personality: vec!["安靜".into()],
        interests: vec!["散步".into()],
        appearance: None,
    }
}

async fn seed(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "INSERT INTO pet_owner(id,name) VALUES('local-owner','本機'),('foreign','其他主人')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO pet_profile(id,name,species,owner_id,is_default_closure,age,image_path) VALUES('a','原名字','tortoise','local-owner',1,7,'keep.png'),('b','其他寵物','tortoise','foreign',0,8,'other.png')").execute(pool).await.unwrap();
    sqlx::query("INSERT INTO pet_extra(id,pet_id,owner_id,category,content) VALUES('old','a','local-owner','喜好','原喜好'),('keep','a','local-owner','其他','保留'),('outside','a','foreign','喜好','其他主人資料')").execute(pool).await.unwrap();
    sqlx::query("INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity) VALUES('act','a','local-owner','sit',1)").execute(pool).await.unwrap();
}

#[sqlx::test(migrations = "../migrations")]
async fn five_fields_roundtrip_preserves_ownership_activity_defaults_and_unrelated_data(
    pool: sqlx::SqlitePool,
) {
    seed(&pool).await;
    let store = SqlitePetSettings(pool.clone());
    let saved = settings::save(&store, "a", profile()).await.unwrap();
    assert_eq!(saved.name, "新名字");
    assert_eq!(saved.likes, vec!["太陽", "草"]);
    assert_eq!(settings::load(&store, "a").await.unwrap(), saved);
    assert!(settings::save(&store, "b", profile()).await.is_err());
    assert!(settings::load(&store, "b").await.is_err());
    assert!(settings::save(&store, "missing", profile()).await.is_err());
    assert!(
        settings::save(
            &store,
            "a",
            SettingsProfile {
                name: "\n".into(),
                ..profile()
            }
        )
        .await
        .is_err()
    );
    let before: (i64, String, bool) =
        sqlx::query_as("SELECT age,image_path,is_default_closure FROM pet_profile WHERE id='a'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, (7, "keep.png".into(), true));
    let foreign: String = sqlx::query_scalar("SELECT name FROM pet_profile WHERE id='b'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(foreign, "其他寵物");
    let kept: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pet_extra WHERE id IN ('keep','outside')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kept, 2);
    let active: bool = sqlx::query_scalar("SELECT is_activity FROM pet_activity WHERE id='act'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(active);
    sqlx::raw_sql("CREATE TRIGGER fail_settings BEFORE INSERT ON pet_extra BEGIN SELECT RAISE(ABORT,'settings ixture'); END;").execute(&pool).await.unwrap();
    assert!(
        settings::save(
            &store,
            "a",
            SettingsProfile {
                name: "不可半套儲存".into(),
                ..profile()
            }
        )
        .await
        .is_err()
    );
    assert_eq!(settings::load(&store, "a").await.unwrap(), saved);
}
#[sqlx::test]
async fn file_pool_restart_loads_same_saved_five_fields() {
    let file = std::env::temp_dir().join(format!("pet-settings-{}.sqlite", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", file.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("../migrations").run(&pool).await.unwrap();
    seed(&pool).await;
    let p = settings::save(&SqlitePetSettings(pool.clone()), "a", profile())
        .await
        .unwrap();
    pool.close().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    assert_eq!(
        settings::load(&SqlitePetSettings(pool.clone()), "a")
            .await
            .unwrap(),
        p
    );
    pool.close().await;
    std::fs::remove_file(file).unwrap();
}
