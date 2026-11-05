#![cfg(feature = "postgres-integration")]
mod support;
use axum::http::{StatusCode, header};
use support::http::send;

fn app(pool: sqlx::PgPool) -> axum::Router {
    support::app(pool, "fixture-chat", "fixture-pet")
}
#[sqlx::test(migrations = "../migrations")]
async fn persistent_account_wait_blocks_even_correct_password(pool: sqlx::PgPool) {
    analytics_server::bootstrap::initialize(&pool, "lesson-only-password".into())
        .await
        .unwrap();
    let bad = send(
        app(pool.clone()),
        "/login",
        Some("username=admin&password=wrong-password"),
        None,
    )
    .await;
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
    let ok = send(
        app(pool.clone()),
        "/login",
        Some("username=ADMIN&password=lesson-only-password"),
        None,
    )
    .await;
    assert_eq!(ok.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(ok.headers().contains_key(header::RETRY_AFTER));
}

async fn expire(pool: &sqlx::PgPool) {
    sqlx::query("UPDATE uac_login_attempt SET blocked_until='0'")
        .execute(pool)
        .await
        .unwrap();
}

async fn state(pool: &sqlx::PgPool) -> (String, String, String) {
    sqlx::query_as("SELECT failures,next_delay,blocked_until FROM uac_login_attempt")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../migrations")]
async fn reopen_no_extension_double_reset_and_rollback(pool: sqlx::PgPool) {
    use support::http::body_text;
    analytics_server::bootstrap::initialize(&pool, "lesson-only-password".into())
        .await
        .unwrap();
    let bad = send(
        app(pool.clone()),
        "/login",
        Some("username=admin&password=wrong-password"),
        None,
    )
    .await;
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
    let before = state(&pool).await;
    assert_eq!((&before.0, &before.1), (&"1".to_owned(), &"15".to_owned()));
    let options = pool.connect_options();
    pool.close().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(3)
        .connect_with((*options).clone())
        .await
        .unwrap();
    for payload in [
        "username=ADMIN&password=lesson-only-password",
        "username=admin&password=",
    ] {
        let res = send(app(pool.clone()), "/login", Some(payload), None).await;
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        let body = body_text(res).await;
        assert!(body.contains("請等待"));
        assert!(body.contains("<form"));
        assert!(!body.contains("lesson-only-password"));
        assert_eq!(state(&pool).await, before);
    }
    expire(&pool).await;
    assert_eq!(
        send(
            app(pool.clone()),
            "/login",
            Some("username=admin&password=wrong-password"),
            None
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(state(&pool).await.1, "30");
    expire(&pool).await;
    // A failing Session insert must roll back the success reset in the same transaction.
    sqlx::query("CREATE FUNCTION reject_day52_session() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture insert failure'; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_day52 BEFORE INSERT ON uac_login_session FOR EACH ROW EXECUTE FUNCTION reject_day52_session()").execute(&pool).await.unwrap();
    assert_eq!(
        send(
            app(pool.clone()),
            "/login",
            Some("username=admin&password=lesson-only-password"),
            None
        )
        .await
        .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(state(&pool).await.0, "2");
    sqlx::query("DROP TRIGGER reject_day52 ON uac_login_session")
        .execute(&pool)
        .await
        .unwrap();
    let ok = send(
        app(pool.clone()),
        "/login",
        Some("username=admin&password=lesson-only-password"),
        None,
    )
    .await;
    assert_eq!(ok.status(), StatusCode::SEE_OTHER);
    assert_eq!(ok.headers()[header::LOCATION], "/change-password");
    assert_eq!(state(&pool).await, ("0".into(), "0".into(), "0".into()));
    sqlx::query("UPDATE uac_user SET is_init_pd=FALSE")
        .execute(&pool)
        .await
        .unwrap();
    let ready = send(
        app(pool.clone()),
        "/login",
        Some("username=admin&password=lesson-only-password"),
        None,
    )
    .await;
    assert_eq!(ready.headers()[header::LOCATION], "/dashboard");
}

#[sqlx::test(migrations = "../migrations")]
async fn concurrent_failures_are_one_failure_plus_one_pending(pool: sqlx::PgPool) {
    analytics_server::bootstrap::initialize(&pool, "lesson-only-password".into())
        .await
        .unwrap();
    let a = send(
        app(pool.clone()),
        "/login",
        Some("username=admin&password=wrong-password"),
        None,
    );
    let b = send(
        app(pool.clone()),
        "/login",
        Some("username=ADMIN&password=wrong-password"),
        None,
    );
    let (a, b) = tokio::join!(a, b);
    let mut statuses = vec![a.status().as_u16(), b.status().as_u16()];
    statuses.sort();
    assert_eq!(statuses, vec![401, 429]);
    assert_eq!(state(&pool).await.0, "1");
    // Separate connections contend on the actual account lock, not a process-local map.
    let mut held = analytics_persistence::login_attempt::LoginAttempt::begin(&pool, "admin")
        .await
        .unwrap()
        .unwrap();
    let other = analytics_persistence::login_attempt::LoginAttempt::begin(&pool, "ADMIN");
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), other)
            .await
            .is_err()
    );
    let _ = held.now().await.unwrap();
    held.commit().await.unwrap();
}
