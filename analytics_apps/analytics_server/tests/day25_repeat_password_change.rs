#![cfg(feature = "postgres-integration")]

use analytics_server::{IngestState, IngestTokens, password_login};
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
            chat: "fixture-chat".into(),
            pet: "fixture-pet".into(),
        }),
    })
}
async fn request(
    app: axum::Router,
    method: &str,
    path: &str,
    form: Option<String>,
    cookie: Option<&str>,
) -> axum::response::Response {
    let mut builder = if method == "POST" {
        Request::post(path)
            .header(header::ORIGIN, "http://127.0.0.1:3100")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
    } else {
        Request::get(path)
    };
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    app.oneshot(
        builder
            .body(form.map_or_else(Body::empty, Body::from))
            .unwrap(),
    )
    .await
    .unwrap()
}
fn cookie(response: &axum::response::Response) -> String {
    response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}
async fn csrf(response: axum::response::Response) -> String {
    let html = String::from_utf8(
        to_bytes(response.into_body(), 16000)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    html.split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .to_owned()
}

#[sqlx::test(migrations = "../migrations")]
async fn day25_admin_can_change_password_again_after_initialization(pool: sqlx::PgPool) {
    analytics_server::bootstrap::initialize(&pool, "lesson-first-password".into())
        .await
        .unwrap();
    let first = request(
        app(pool.clone()),
        "POST",
        "/login",
        Some("username=admin&password=lesson-first-password".into()),
        None,
    )
    .await;
    assert_eq!(first.status(), StatusCode::SEE_OTHER);
    assert_eq!(first.headers()[header::LOCATION], "/change-password");
    let c1 = cookie(&first);
    let initial_page = request(
        app(pool.clone()),
        "GET",
        "/change-password",
        None,
        Some(&c1),
    )
    .await;
    assert_eq!(initial_page.status(), StatusCode::OK);
    let token1 = csrf(initial_page).await;
    let initial_form = format!(
        "current_password=lesson-first-password&new_password=lesson-second-password&confirm_password=lesson-second-password&email=first%40example.test&csrf={token1}"
    );
    let changed1 = request(
        app(pool.clone()),
        "POST",
        "/change-password",
        Some(initial_form),
        Some(&c1),
    )
    .await;
    assert_eq!(changed1.status(), StatusCode::SEE_OTHER);

    let second_login = request(
        app(pool.clone()),
        "POST",
        "/login",
        Some("username=admin&password=lesson-second-password".into()),
        None,
    )
    .await;
    assert_eq!(second_login.status(), StatusCode::SEE_OTHER);
    assert_eq!(second_login.headers()[header::LOCATION], "/dashboard");
    let c2 = cookie(&second_login);
    // Old main returns 303 to /dashboard instead of rendering change form.
    let second_page = request(
        app(pool.clone()),
        "GET",
        "/change-password",
        None,
        Some(&c2),
    )
    .await;
    assert_eq!(second_page.status(), StatusCode::OK);
    let token2 = csrf(second_page).await;
    let next_form = format!(
        "current_password=lesson-second-password&new_password=lesson-third-password&confirm_password=lesson-third-password&email=second%40example.test&csrf={token2}"
    );
    let changed2 = request(
        app(pool.clone()),
        "POST",
        "/change-password",
        Some(next_form),
        Some(&c2),
    )
    .await;
    assert_eq!(changed2.status(), StatusCode::SEE_OTHER);
    assert_eq!(changed2.headers()[header::LOCATION], "/login");
    let stored_email: String =
        sqlx::query_scalar("SELECT user_email FROM uac_user WHERE lower(user_acnt)='admin'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored_email, "second@example.test");
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM uac_login_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sessions, 0);
    assert_eq!(
        request(
            app(pool.clone()),
            "POST",
            "/login",
            Some("username=admin&password=lesson-second-password".into()),
            None
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    // Test-only clock advance before the existing corrected-password assertion.
    sqlx::query("UPDATE uac_login_attempt SET blocked_until='0'")
        .execute(&pool)
        .await
        .unwrap();
    let third = request(
        app(pool),
        "POST",
        "/login",
        Some("username=admin&password=lesson-third-password".into()),
        None,
    )
    .await;
    assert_eq!(third.status(), StatusCode::SEE_OTHER);
    assert_eq!(third.headers()[header::LOCATION], "/dashboard");
}
