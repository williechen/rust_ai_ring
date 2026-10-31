use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
#[tokio::test]
async fn actual_signup_default_role_reaches_self_service_without_fixture_grants() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    chat_persistence::migrate(&pool).await.unwrap();
    let app =
        chat_server::signup::routes(pool.clone()).merge(chat_server::signin::routes(pool.clone()));
    let signup=app.clone().oneshot(Request::post("/signup").header("origin","http://127.0.0.1:3000").header("content-type","application/x-www-form-urlencoded").body(Body::from("username=real_signup&email=real%40example.test&display_name=Real&password=signup-password")).unwrap()).await.unwrap();
    assert_eq!(signup.status(), StatusCode::SEE_OTHER);
    let exceptions: i64 = sqlx::query_scalar("SELECT count(*) FROM mbr_user_perm")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        exceptions, 0,
        "no personal fixture grants may mask role wiring"
    );
    let login = app
        .clone()
        .oneshot(
            Request::post("/login")
                .header("origin", "http://127.0.0.1:3000")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("username=real_signup&password=signup-password"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
    let cookie = login.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    for path in ["/account/profile", "/account/password"] {
        let response = app
            .clone()
            .oneshot(
                Request::get(path)
                    .header("cookie", cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "actual registered user's default-role self action: {path}"
        );
    }
}
