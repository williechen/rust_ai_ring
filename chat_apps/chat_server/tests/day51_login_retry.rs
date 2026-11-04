use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;

fn form(path: &str, data: String) -> Request<Body> {
    Request::post(path)
        .header("origin", "http://127.0.0.1:3000")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(data))
        .unwrap()
}

#[tokio::test]
async fn failed_login_keeps_safe_room_and_username_then_retry_creates_one_session() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    chat_persistence::migrate(&pool).await.unwrap();
    let signup = chat_server::signup::routes(pool.clone());
    let response=signup.oneshot(form("/signup", "username=learner51&email=learner51%40example.test&display_name=learner&password=correct-password".into())).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let app = chat_server::signin::routes(pool.clone());
    let next = "/room/00000000-0000-0000-0000-000000000051";
    let response = app
        .clone()
        .oneshot(form(
            "/login",
            format!("username=learner51&password=wrong-secret&next={next}"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(!response.headers().contains_key("set-cookie"));
    let html = String::from_utf8(
        to_bytes(response.into_body(), 20000)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(html.contains("帳號或密碼不正確"));
    assert!(html.contains("action=\"/login\""));
    assert!(html.contains("value=\"learner51\""));
    assert!(html.contains(&format!("value=\"{next}\"")));
    assert!(!html.contains("wrong-secret"));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mbr_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let response = app
        .clone()
        .oneshot(form(
            "/login",
            format!("username=learner51&password=correct-password&next={next}"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()["location"], next);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mbr_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let response = app
        .oneshot(form(
            "/login",
            "username=%3Cscript%3E&password=wrong-secret&next=https%3A%2F%2Fevil.invalid".into(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let html = String::from_utf8(
        to_bytes(response.into_body(), 20000)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(html.contains("帳號或密碼不正確"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("<script>"));
    assert!(!html.contains("evil.invalid"));
    assert!(!html.contains("wrong-secret"));
}
