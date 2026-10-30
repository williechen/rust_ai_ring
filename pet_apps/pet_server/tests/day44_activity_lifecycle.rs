use pet_persistence::activity::{self, SIT, WALK};
use pet_server::pet_store::PetStore;

async fn flags(pool: &sqlx::SqlitePool, id: &str) -> Vec<(String, Option<i64>)> {
    sqlx::query_as("SELECT activity_type,is_activity FROM pet_activity WHERE pet_id=?1 AND owner_id='local-owner' ORDER BY activity_type,id")
        .bind(id).fetch_all(pool).await.unwrap()
}

#[sqlx::test(migrations = "../migrations")]
async fn creation_has_two_inactive_activities_and_no_new_columns(pool: sqlx::SqlitePool) {
    let store = PetStore { pool: pool.clone() };
    let p = store.create("A").await.unwrap();
    assert_eq!(
        flags(&pool, &p.id).await,
        vec![(SIT.into(), Some(0)), (WALK.into(), Some(0))]
    );
    assert!(store.read_caged(&p.id).await.unwrap());
    assert!(store.activity_status(&p.id).await.unwrap().default_caged);
    assert_eq!(
        sqlx::query("PRAGMA table_info(pet_profile)")
            .fetch_all(&pool)
            .await
            .unwrap()
            .len(),
        11
    );
    assert_eq!(
        sqlx::query("PRAGMA table_info(pet_activity)")
            .fetch_all(&pool)
            .await
            .unwrap()
            .len(),
        8
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn failure_creating_second_activity_rolls_back_owner_profile_and_first_activity(
    pool: sqlx::SqlitePool,
) {
    sqlx::query("CREATE TRIGGER reject_sit BEFORE INSERT ON pet_activity WHEN NEW.activity_type='坐下' BEGIN SELECT RAISE(ABORT,'injected failure'); END")
        .execute(&pool).await.unwrap();
    let store = PetStore { pool: pool.clone() };
    assert!(store.create("A").await.is_err());
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM pet_owner),(SELECT count(*) FROM pet_profile),(SELECT count(*) FROM pet_activity)").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
}

#[sqlx::test(migrations = "../migrations")]
async fn any_active_type_means_outside_and_sit_wins_and_persists(pool: sqlx::SqlitePool) {
    let store = PetStore { pool: pool.clone() };
    let p = store.create("A").await.unwrap();
    sqlx::query("INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity) VALUES('other',?1,'local-owner','玩耍',1)").bind(&p.id).execute(&pool).await.unwrap();
    let s = store.activity_status(&p.id).await.unwrap();
    assert!(s.outside);
    assert!(!s.roaming);
    assert!(!store.read_caged(&p.id).await.unwrap());
    store.switch_activity(&p.id, true).await.unwrap();
    assert!(store.activity_status(&p.id).await.unwrap().roaming);
    sqlx::query("UPDATE pet_activity SET is_activity=1 WHERE pet_id=?1 AND activity_type='坐下'")
        .bind(&p.id)
        .execute(&pool)
        .await
        .unwrap();
    let s = store.activity_status(&p.id).await.unwrap();
    assert!(s.outside);
    assert!(!s.roaming);
    let walking:i64=sqlx::query_scalar("SELECT count(*) FROM pet_activity WHERE pet_id=?1 AND activity_type='散步' AND is_activity=1").bind(&p.id).fetch_one(&pool).await.unwrap();
    assert_eq!(walking, 0);
    assert!(store.activity_status(&p.id).await.unwrap().default_caged);
}

#[sqlx::test(migrations = "../migrations")]
async fn repeats_do_not_duplicate_and_cage_stops_only_target_without_changing_defaults(
    pool: sqlx::SqlitePool,
) {
    let store = PetStore { pool: pool.clone() };
    let a = store.create("A").await.unwrap();
    let b = store.create("B").await.unwrap();
    store.set_default_caged(&a.id, false).await.unwrap();
    store.switch_activity(&a.id, false).await.unwrap();
    store.switch_activity(&b.id, true).await.unwrap();
    for _ in 0..3 {
        store.select(&a.id).await.unwrap();
        store.set_caged(&a.id, false).await.unwrap();
    }
    assert_eq!(flags(&pool, &a.id).await.len(), 2);
    assert!(!store.activity_status(&a.id).await.unwrap().roaming);
    sqlx::query("INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity) VALUES('play',?1,'local-owner','玩耍',1)").bind(&a.id).execute(&pool).await.unwrap();
    store.set_caged(&a.id, true).await.unwrap();
    assert!(store.read_caged(&a.id).await.unwrap());
    assert!(!store.read_caged(&b.id).await.unwrap());
    assert!(!store.activity_status(&a.id).await.unwrap().default_caged);
    assert_eq!(store.default_pets().await.unwrap(), vec![a]);
}

#[sqlx::test(migrations = "../migrations")]
async fn missing_and_foreign_pet_fail_closed_and_owner_mismatch_does_not_leak(
    pool: sqlx::SqlitePool,
) {
    let store = PetStore { pool: pool.clone() };
    let a = store.create("A").await.unwrap();
    sqlx::query("INSERT INTO pet_owner(id,name) VALUES('other-owner','Other')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO pet_profile(id,name,species,owner_id) VALUES('foreign','X','tortoise','other-owner')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity) VALUES('mismatch',?1,'other-owner','散步',1)").bind(&a.id).execute(&pool).await.unwrap();
    assert!(store.read_caged(&a.id).await.unwrap());
    for id in ["foreign", "missing"] {
        assert!(store.read_caged(id).await.is_err());
        assert!(store.switch_activity(id, true).await.is_err());
        assert!(store.set_default_caged(id, false).await.is_err());
    }
    assert!(store.uncaged().await.unwrap().is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn native_failure_restores_exact_sitting_and_other_activity_snapshot(pool: sqlx::SqlitePool) {
    let store = PetStore { pool: pool.clone() };
    let a = store.create("A").await.unwrap();
    store.select(&a.id).await.unwrap();
    store.switch_activity(&a.id, false).await.unwrap();
    sqlx::query("INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity) VALUES('play',?1,'local-owner','玩耍',1)").bind(&a.id).execute(&pool).await.unwrap();
    let before = flags(&pool, &a.id).await;
    let snapshot = store.snapshot(&a.id).await.unwrap();
    let result = pet_application::lifecycle::persist_then_apply(
        || store.set_caged(&a.id, true),
        || Err("native close failure".into()),
        || store.restore(&a.id, &snapshot),
    )
    .await;
    assert!(result.unwrap_err().contains("native close failure"));
    assert_eq!(flags(&pool, &a.id).await, before);
    let s = store.activity_status(&a.id).await.unwrap();
    assert!(s.outside);
    assert!(!s.roaming);
    assert!(s.default_caged);
}

#[sqlx::test(migrations = "../migrations")]
async fn read_and_normalization_failure_never_reports_success(pool: sqlx::SqlitePool) {
    let store = PetStore { pool: pool.clone() };
    let a = store.create("A").await.unwrap();
    sqlx::query("UPDATE pet_activity SET is_activity=1 WHERE pet_id=?1")
        .bind(&a.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER reject_normalization BEFORE UPDATE ON pet_activity BEGIN SELECT RAISE(ABORT,'db failure'); END").execute(&pool).await.unwrap();
    assert!(store.activity_status(&a.id).await.is_err());
    pool.close().await;
    assert!(activity::status(&pool, &a.id).await.is_err());
}

#[sqlx::test(migrations = "../migrations")]
async fn startup_uses_existing_defaults_and_cages_non_default_activities(pool: sqlx::SqlitePool) {
    let store = PetStore { pool: pool.clone() };
    let a = store.create("A").await.unwrap();
    let b = store.create("B").await.unwrap();
    store.set_default_caged(&a.id, false).await.unwrap();
    store.switch_activity(&a.id, false).await.unwrap();
    store.switch_activity(&b.id, true).await.unwrap();
    // Caging A does not clear its startup preference; B is active but not a default pet.
    store.set_caged(&a.id, true).await.unwrap();
    let restarted = PetStore { pool: pool.clone() };
    let defaults = restarted.prepare_startup().await.unwrap();
    assert_eq!(defaults, vec![a.clone()]);
    assert!(restarted.read_caged(&b.id).await.unwrap());
    for pet in defaults {
        restarted.set_caged(&pet.id, false).await.unwrap();
    }
    assert_eq!(restarted.uncaged().await.unwrap(), vec![a]);
}

#[sqlx::test(migrations = "../migrations")]
async fn default_change_and_normalization_are_one_transaction(pool: sqlx::SqlitePool) {
    let store = PetStore { pool: pool.clone() };
    let a = store.create("A").await.unwrap();
    sqlx::query("UPDATE pet_activity SET is_activity=1 WHERE pet_id=?1")
        .bind(&a.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER reject_walk_update BEFORE UPDATE ON pet_activity WHEN OLD.activity_type='散步' BEGIN SELECT RAISE(ABORT,'normalization fails'); END").execute(&pool).await.unwrap();
    assert!(store.set_default_caged(&a.id, false).await.is_err());
    let default: i64 = sqlx::query_scalar("SELECT is_default_closure FROM pet_profile WHERE id=?1")
        .bind(&a.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(default, 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn failed_mode_validation_rolls_back_both_activity_changes(pool: sqlx::SqlitePool) {
    let store = PetStore { pool: pool.clone() };
    let a = store.create("A").await.unwrap();
    store.switch_activity(&a.id, false).await.unwrap();
    let before = flags(&pool, &a.id).await;
    sqlx::query("UPDATE pet_profile SET is_default_closure=2 WHERE id=?1")
        .bind(&a.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(store.switch_activity(&a.id, true).await.is_err());
    assert_eq!(flags(&pool, &a.id).await, before);
}
