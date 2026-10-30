#![cfg(feature = "postgres-integration")]
mod support;

use analytics_server::{IngestState, IngestTokens, bootstrap, password_login};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use std::sync::Arc;
use support::http::{body_text, response_cookie, send};
use tower::ServiceExt;

#[sqlx::test(migrations = "../migrations")]
async fn real_pg_logout_requires_session_bound_form_token(pool: sqlx::PgPool) {
    bootstrap::initialize(&pool, "lesson-test-password".into())
        .await
        .unwrap();
    let app = password_login::build(IngestState {
        pool: pool.clone(),
        tokens: Arc::new(IngestTokens {
            chat: "chat-fixture".into(),
            pet: "pet-fixture".into(),
        }),
    });
    let login = send(
        app.clone(),
        "/login",
        Some("username=admin&password=lesson-test-password"),
        None,
    )
    .await;
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
    let cookie = response_cookie(&login);
    let page = send(app.clone(), "/change-password", None, Some(&cookie)).await;
    assert_eq!(page.status(), StatusCode::OK);
    let html = body_text(page).await;
    let logout_form = html.split("action=\"/logout\"").nth(1).unwrap();
    let csrf = logout_form
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    assert_eq!(csrf.len(), 64);
    assert_eq!(
        send(app.clone(), "/logout", Some("csrf=wrong"), Some(&cookie))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let still_active: i64 = sqlx::query_scalar("SELECT count(*) FROM uac_login_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(still_active, 1);
    let foreign = app
        .clone()
        .oneshot(
            Request::post("/logout")
                .header(header::ORIGIN, "https://foreign.example")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie)
                .body(Body::from(format!("csrf={csrf}")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
    let logout = send(
        app.clone(),
        "/logout",
        Some(&format!("csrf={csrf}")),
        Some(&cookie),
    )
    .await;
    assert_eq!(logout.status(), StatusCode::SEE_OTHER);
    assert_eq!(logout.headers()[header::LOCATION], "/login");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM uac_login_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        send(app, "/change-password", None, Some(&cookie))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}
