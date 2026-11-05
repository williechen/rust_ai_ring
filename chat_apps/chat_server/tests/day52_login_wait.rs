use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;

fn request(password: &str) -> Request<Body> {
    Request::post("/login")
        .header("origin", "http://127.0.0.1:3000")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(format!(
            "username=WAIT52&password={password}&next=/room/00000000-0000-0000-0000-000000000052"
        )))
        .unwrap()
}

#[tokio::test]
async fn persistent_account_wait_blocks_correct_password_without_extending_and_success_clears() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    chat_persistence::migrate(&pool).await.unwrap();
    let signup = chat_server::signup::routes(pool.clone());
    let response=signup.oneshot(Request::post("/signup").header("origin","http://127.0.0.1:3000").header("content-type","application/x-www-form-urlencoded").body(Body::from("username=wait52&email=wait52%40example.test&display_name=wait&password=correct-password")).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let app = chat_server::signin::routes(pool.clone());
    let bad = app
        .clone()
        .oneshot(request("wrong-password"))
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
    let wait = app
        .clone()
        .oneshot(request("correct-password"))
        .await
        .unwrap();
    assert_eq!(wait.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(wait.headers().contains_key("retry-after"));
    assert!(!wait.headers().contains_key("set-cookie"));
    let html =
        String::from_utf8(to_bytes(wait.into_body(), 20000).await.unwrap().to_vec()).unwrap();
    assert!(html.contains("尚須等待"));
    assert!(html.contains("/room/00000000-0000-0000-0000-000000000052"));
    assert!(!html.contains("correct-password"));
    let before: (String, String, String) =
        sqlx::query_as("SELECT failures,next_delay,blocked_until FROM mbr_login_attempt")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before.0, "1");
    assert_eq!(before.1, "15");
    let fresh_router = chat_server::signin::routes(pool.clone());
    assert_eq!(
        fresh_router
            .oneshot(request("wrong-password"))
            .await
            .unwrap()
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );

    assert_eq!(
        app.clone().oneshot(request("")).await.unwrap().status(),
        StatusCode::TOO_MANY_REQUESTS
    );

    let after: (String, String, String) =
        sqlx::query_as("SELECT failures,next_delay,blocked_until FROM mbr_login_attempt")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    sqlx::query("UPDATE mbr_login_attempt SET blocked_until='0'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(request("wrong-password"))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let delay: String = sqlx::query_scalar("SELECT next_delay FROM mbr_login_attempt")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(delay, "30");
    sqlx::query("UPDATE mbr_login_attempt SET blocked_until='0'")
        .execute(&pool)
        .await
        .unwrap();
    let ok = app.oneshot(request("correct-password")).await.unwrap();
    assert_eq!(ok.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        ok.headers()["location"],
        "/room/00000000-0000-0000-0000-000000000052"
    );
    let state: (String, String, String) =
        sqlx::query_as("SELECT failures,next_delay,blocked_until FROM mbr_login_attempt")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, ("0".into(), "0".into(), "0".into()));
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM mbr_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sessions, 1);
}

#[tokio::test]
async fn file_database_reopen_preserves_wait_and_failed_session_insert_rolls_back_reset() {
    use chat_application::login_attempt::{AttemptStore, accepted, pending, reject};
    use chat_persistence::login_attempt::LoginAttempt;
    let path = std::env::temp_dir().join(format!("chat-wait-{}.sqlite", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    chat_persistence::migrate(&pool).await.unwrap();
    sqlx::query("INSERT INTO mbr_user(id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at) VALUES('persist52','persist52','unused-storage-fixture','persist52@example.test','持久化測試','active',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
    let mut attempt = LoginAttempt::begin(&pool, "PERSIST52")
        .await
        .unwrap()
        .unwrap();
    let now = attempt.now().await.unwrap();
    assert_eq!(reject(&mut attempt, now).await.unwrap(), "15");
    attempt.commit().await.unwrap();
    pool.close().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let mut attempt = LoginAttempt::begin(&pool, "persist52")
        .await
        .unwrap()
        .unwrap();
    let now = attempt.now().await.unwrap();
    assert_ne!(pending(&mut attempt, now).await.unwrap(), "0");
    assert_eq!(attempt.read().await.unwrap().failures, "1");
    attempt.commit().await.unwrap();
    sqlx::query("UPDATE mbr_login_attempt SET blocked_until='0'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TRIGGER fail_session52 BEFORE INSERT ON mbr_session BEGIN SELECT RAISE(ABORT,'storage fixture'); END;").execute(&pool).await.unwrap();
    let mut attempt = LoginAttempt::begin(&pool, "persist52")
        .await
        .unwrap()
        .unwrap();
    accepted(&mut attempt).await.unwrap();
    assert!(attempt.session("fault52", &"e".repeat(64)).await.is_err());
    let state: (String, String) = sqlx::query_as(
        "SELECT failures,next_delay FROM mbr_login_attempt WHERE user_id='persist52'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, ("1".into(), "15".into()));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mbr_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    pool.close().await;
    std::fs::remove_file(path).unwrap();
}
