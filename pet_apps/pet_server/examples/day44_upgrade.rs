use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(&args[1])
            .create_if_missing(true),
    )
    .await?;
    if args.get(4).is_some_and(|s| s == "seed") {
        sqlx::migrate::Migrator::new(std::path::Path::new(&args[2]))
            .await?
            .run(&pool)
            .await?;
        sqlx::query("INSERT INTO pet_owner(id,name) VALUES('local-owner','驗收主人')")
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO pet_profile(id,name,species,owner_id,is_default_closure) VALUES('old-out','Old Out','tortoise','local-owner',0),('old-in','Old In','tortoise','local-owner',1),('old-sit','Old Sit','tortoise','local-owner',0)").execute(&pool).await?;
        sqlx::query("INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity) VALUES('legacy-walk','old-sit','local-owner','散步',1),('legacy-sit','old-sit','local-owner','坐下',1),('legacy-play','old-sit','local-owner','玩耍',1)").execute(&pool).await?;
    }
    let profile_count: i64 = sqlx::query_scalar("SELECT count(*) FROM pet_profile")
        .fetch_one(&pool)
        .await?;
    let schema: Vec<(String, String)> =
        sqlx::query_as("SELECT name,sql FROM sqlite_master WHERE type='table' ORDER BY name")
            .fetch_all(&pool)
            .await?;
    let before: Vec<(String, Option<i64>)> =
        sqlx::query_as("SELECT id,is_default_closure FROM pet_profile ORDER BY id")
            .fetch_all(&pool)
            .await?;
    let m = sqlx::migrate::Migrator::new(std::path::Path::new(&args[3])).await?;
    if args.get(4).is_some_and(|s| s == "tamper") {
        sqlx::query("UPDATE _sqlx_migrations SET checksum=zeroblob(length(checksum)) WHERE version=(SELECT min(version) FROM _sqlx_migrations)").execute(&pool).await?;
        assert!(m.run(&pool).await.is_err());
        assert_eq!(
            before,
            sqlx::query_as::<_, (String, Option<i64>)>(
                "SELECT id,is_default_closure FROM pet_profile ORDER BY id"
            )
            .fetch_all(&pool)
            .await?
        );
        println!("checksum drift rejected and profile preferences preserved");
        return Ok(());
    }
    m.run(&pool).await?;
    m.run(&pool).await?;
    assert_eq!(
        schema,
        sqlx::query_as::<_, (String, String)>(
            "SELECT name,sql FROM sqlite_master WHERE type='table' ORDER BY name"
        )
        .fetch_all(&pool)
        .await?
    );
    assert_eq!(
        before,
        sqlx::query_as::<_, (String, Option<i64>)>(
            "SELECT id,is_default_closure FROM pet_profile ORDER BY id"
        )
        .fetch_all(&pool)
        .await?
    );
    let missing:i64=sqlx::query_scalar("SELECT count(*) FROM pet_profile p WHERE NOT EXISTS(SELECT 1 FROM pet_activity a WHERE a.pet_id=p.id AND a.owner_id=p.owner_id AND a.activity_type='散步') OR NOT EXISTS(SELECT 1 FROM pet_activity a WHERE a.pet_id=p.id AND a.owner_id=p.owner_id AND a.activity_type='坐下')").fetch_one(&pool).await?;
    assert_eq!(missing, 0);
    let conflicts:i64=sqlx::query_scalar("SELECT count(*) FROM pet_activity w WHERE w.activity_type='散步' AND w.is_activity=1 AND EXISTS(SELECT 1 FROM pet_activity s WHERE s.pet_id=w.pet_id AND s.owner_id=w.owner_id AND s.activity_type='坐下' AND s.is_activity=1)").fetch_one(&pool).await?;
    assert_eq!(conflicts, 0);
    if args.get(4).is_some_and(|s| s == "seed") {
        let out = pet_persistence::activity::status(&pool, "old-out").await?;
        assert!(out.outside && out.roaming);
        let caged = pet_persistence::activity::status(&pool, "old-in").await?;
        assert!(!caged.outside);
        let sit = pet_persistence::activity::status(&pool, "old-sit").await?;
        assert!(sit.outside && !sit.roaming);
        let play: i64 =
            sqlx::query_scalar("SELECT is_activity FROM pet_activity WHERE id='legacy-play'")
                .fetch_one(&pool)
                .await?;
        assert_eq!(play, 1);
    }
    println!(
        "upgrade/idempotence passed; profiles={profile_count}; profile defaults unchanged; schema/columns unchanged; two basic activities present; sit priority preserved"
    );
    pool.close().await;
    Ok(())
}
