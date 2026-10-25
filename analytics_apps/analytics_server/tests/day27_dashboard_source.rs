// apps/analytics_server/tests/day27_dashboard_source.rs
#![cfg(feature = "postgres-integration")]
use analytics_server::{IngestState, IngestTokens, bootstrap, password_login};
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use std::sync::Arc;
use tower::ServiceExt;

fn app(pool: sqlx::PgPool) -> axum::Router {
    password_login::build(IngestState {
        pool,
        tokens: Arc::new(IngestTokens {
            chat: "test-chat".into(),
            pet: "test-pet".into(),
        }),
    })
}
async fn get(app: axum::Router, path: &str, cookie: Option<&str>) -> axum::response::Response {
    let mut request = Request::get(path);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}
async fn post(
    app: axum::Router,
    path: &str,
    form: String,
    cookie: Option<&str>,
) -> axum::response::Response {
    let mut req = Request::post(path)
        .header(header::ORIGIN, "http://127.0.0.1:3100")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        req = req.header(header::COOKIE, cookie);
    }
    app.oneshot(req.body(Body::from(form)).unwrap())
        .await
        .unwrap()
}
fn response_cookie(r: &axum::response::Response) -> String {
    r.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}
async fn body_text(r: axum::response::Response) -> String {
    String::from_utf8(to_bytes(r.into_body(), 32768).await.unwrap().to_vec()).unwrap()
}
#[sqlx::test(migrations = "../migrations")]
async fn day27_session_admin_can_switch_real_source_counts(pool: sqlx::PgPool) {
    bootstrap::initialize(&pool, "first-password-27".into())
        .await
        .unwrap();
    for (source, event_type) in [
        ("web_chat", "chat.message.persisted"),
        ("desktop_pet", "pet.cleanup.succeeded"),
        ("desktop_pet", "pet.sleep.started"),
    ] {
        sqlx::query(
            "INSERT INTO product_events(source_app,event_id,schema_version,event_type)
             VALUES($1,$2,1,$3)",
        )
        .bind(source)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(event_type)
        .execute(&pool)
        .await
        .unwrap();
    }
    let denied = get(app(pool.clone()), "/dashboard?source=desktop_pet", None).await;
    assert_eq!(denied.status(), StatusCode::SEE_OTHER);
    assert_eq!(denied.headers()[header::LOCATION], "/login");

    let initial = post(
        app(pool.clone()),
        "/login",
        "username=admin&password=first-password-27".into(),
        None,
    )
    .await;
    assert_eq!(initial.status(), StatusCode::SEE_OTHER);
    let first_cookie = response_cookie(&initial);
    let restricted = get(
        app(pool.clone()),
        "/dashboard?source=desktop_pet",
        Some(&first_cookie),
    )
    .await;
    assert_eq!(restricted.status(), StatusCode::SEE_OTHER);
    assert_eq!(restricted.headers()[header::LOCATION], "/change-password");

    let change = get(app(pool.clone()), "/change-password", Some(&first_cookie)).await;
    assert_eq!(change.status(), StatusCode::OK);
    let html = body_text(change).await;
    let csrf = html
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let changed=post(app(pool.clone()),"/change-password",
        format!("current_password=first-password-27&new_password=next-password-27&confirm_password=next-password-27&email=day27%40example.test&csrf={csrf}"),
        Some(&first_cookie)).await;
    assert_eq!(changed.status(), StatusCode::SEE_OTHER);

    let login = post(
        app(pool.clone()),
        "/login",
        "username=admin&password=next-password-27".into(),
        None,
    )
    .await;
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
    assert_eq!(login.headers()[header::LOCATION], "/dashboard");
    let cookie = response_cookie(&login);

    let chat = get(app(pool.clone()), "/dashboard", Some(&cookie)).await;
    assert_eq!(chat.status(), StatusCode::OK);
    let chat_html = body_text(chat).await;
    assert!(chat_html.contains("全部事件類型：1 筆已提交事件"));

    let pet = get(
        app(pool.clone()),
        "/dashboard?source=desktop_pet",
        Some(&cookie),
    )
    .await;
    assert_eq!(pet.status(), StatusCode::OK);
    let pet_html = body_text(pet).await;
    assert!(pet_html.contains("全部事件類型：2 筆已提交事件"));
    assert!(pet_html.contains("/dashboard?source=web_chat"));

    let forbidden = get(app(pool), "/dashboard?source=unknown", Some(&cookie)).await;
    assert_eq!(forbidden.status(), StatusCode::BAD_REQUEST);
}
