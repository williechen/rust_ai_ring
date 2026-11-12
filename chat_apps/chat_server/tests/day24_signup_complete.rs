use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;
#[tokio::test]
async fn day24_signup_completes_without_automatic_login_or_navigation() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    for sql in [
        include_str!("../../migrations/20261018073931_create_auth_user.sql"),
        include_str!("../../migrations/20261019221857_member_identity.sql"),
        include_str!("../../migrations/20261020152737_mbr_sessions.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    let app = chat_server::signup::routes(pool.clone());
    let response=app.clone().oneshot(Request::post("/signup").header("origin","http://127.0.0.1:3000").header("content-type","application/x-www-form-urlencoded").body(Body::from("username=day24&email=day24%40example.test&display_name=Learner&password=lesson-only-password")).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()["location"], "/signup/complete");
    assert!(
        response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mbr_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let page = app
        .clone()
        .oneshot(
            Request::get("/signup/complete")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    assert!(!page.headers().contains_key("location"));
    let html = String::from_utf8(
        to_bytes(page.into_body(), 1_000_000)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(html.contains("註冊完成"));
    assert!(html.contains("目前保持登出"));
    assert!(html.contains(r#"href="/""#));
    assert!(!html.to_lowercase().contains(r#"http-equiv="refresh""#));
    assert!(!html.contains("<script"));
    let duplicate=app.oneshot(Request::post("/signup").header("origin","http://127.0.0.1:3000").header("content-type","application/x-www-form-urlencoded").body(Body::from("username=DAY24&email=other%40example.test&display_name=Learner&password=lesson-only-password")).unwrap()).await.unwrap();
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn day24_existing_browser_session_revocation_is_atomic() {
    use sha2::{Digest, Sha256};
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    for sql in [
        include_str!("../../migrations/20261018073931_create_auth_user.sql"),
        include_str!("../../migrations/20261019221857_member_identity.sql"),
        include_str!("../../migrations/20261020152737_mbr_sessions.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    let app = chat_server::signup::routes(pool.clone());
    let make = |name: &str, cookie: Option<String>| {
        let mut req = Request::post("/signup")
            .header("origin", "http://127.0.0.1:3000")
            .header("content-type", "application/x-www-form-urlencoded");
        if let Some(c) = cookie {
            req = req.header("cookie", c);
        }
        req.body(Body::from(format!("username={name}&email={name}%40example.test&display_name=Learner&password=lesson-only-password"))).unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(make("older", None))
            .await
            .unwrap()
            .status(),
        StatusCode::SEE_OTHER
    );
    let raw = "a".repeat(64);
    let hash: String = Sha256::digest(raw.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    sqlx::query("INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at) SELECT 'old-session',id,?1,unixepoch()+1000 FROM mbr_user WHERE user_acnt='older'").bind(hash).execute(&pool).await.unwrap();
    sqlx::raw_sql("CREATE TRIGGER test_revoke_failure BEFORE DELETE ON mbr_session BEGIN SELECT RAISE(ABORT,'fixture'); END;").execute(&pool).await.unwrap();
    assert_eq!(
        app.clone()
            .oneshot(make("newer", Some(format!("chat_session={raw}"))))
            .await
            .unwrap()
            .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mbr_user WHERE user_acnt='newer'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("DROP TRIGGER test_revoke_failure")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.oneshot(make("newer", Some(format!("chat_session={raw}"))))
            .await
            .unwrap()
            .status(),
        StatusCode::SEE_OTHER
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mbr_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
