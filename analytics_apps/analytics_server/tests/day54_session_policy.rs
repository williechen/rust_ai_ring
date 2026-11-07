#![cfg(feature = "postgres-integration")]
mod support;
use axum::http::{StatusCode, header};
use sha2::{Digest, Sha256};
use support::http::{body_text, response_cookie, send};

fn app(pool: sqlx::PgPool) -> axum::Router {
    support::app(pool, "fixture-chat", "fixture-pet")
}

fn csrf(cookie: &str) -> String {
    let raw = cookie.split_once('=').unwrap().1;
    format!(
        "{:x}",
        Sha256::digest(format!("session-settings:{raw}").as_bytes())
    )
}

#[sqlx::test(migrations = "../migrations")]
async fn day54_idle_setting_is_shared_by_login_refresh_and_cookie(pool: sqlx::PgPool) {
    analytics_server::bootstrap::initialize(&pool, "lesson-only-password".into())
        .await
        .unwrap();
    // 僅隔離 fixture 模擬已完成首次改密碼的管理員。
    sqlx::query("UPDATE uac_user SET is_init_pd=FALSE")
        .execute(&pool)
        .await
        .unwrap();
    let login = send(
        app(pool.clone()),
        "/login",
        Some("username=admin&password=lesson-only-password"),
        None,
    )
    .await;
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
    assert!(
        login.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=1800")
    );
    let cookie = response_cookie(&login);
    let page = send(app(pool.clone()), "/settings/session", None, Some(&cookie)).await;
    assert_eq!(page.status(), StatusCode::OK);
    let html = body_text(page).await;
    assert!(html.contains("管理工作階段閒置秒數"));
    assert!(html.contains("name=\"idle_seconds\""));
    assert!(html.contains("name=\"csrf\""));
    let bad = send(
        app(pool.clone()),
        "/settings/session",
        Some("idle_seconds=300&csrf=bad"),
        Some(&cookie),
    )
    .await;
    assert_eq!(bad.status(), StatusCode::FORBIDDEN);
    let out_of_range = format!("idle_seconds=59&csrf={}", csrf(&cookie));
    let bad = send(
        app(pool.clone()),
        "/settings/session",
        Some(&out_of_range),
        Some(&cookie),
    )
    .await;
    assert_eq!(bad.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let update = format!("idle_seconds=300&csrf={}", csrf(&cookie));
    let changed = send(
        app(pool.clone()),
        "/settings/session",
        Some(&update),
        Some(&cookie),
    )
    .await;
    assert_eq!(changed.status(), StatusCode::SEE_OTHER);
    assert_eq!(changed.headers()[header::LOCATION], "/settings/session");
    assert!(
        changed.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=300")
    );
    let saved: i64 =
        sqlx::query_scalar("SELECT idle_seconds FROM analytics_session_policy WHERE id=1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(saved, 300);
    let refresh = send(app(pool.clone()), "/dashboard", None, Some(&cookie)).await;
    assert_eq!(refresh.status(), StatusCode::OK);
    assert!(
        refresh.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=300")
    );
    let token_hash = format!(
        "{:x}",
        Sha256::digest(cookie.split_once('=').unwrap().1.as_bytes())
    );
    let remaining: i64 = sqlx::query_scalar(
        "SELECT expires_at-floor(extract(epoch from now()))::bigint
         FROM uac_login_session WHERE token_hash=$1",
    )
    .bind(token_hash)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!((290..=300).contains(&remaining));
    let second = send(
        app(pool.clone()),
        "/login",
        Some("username=admin&password=lesson-only-password"),
        None,
    )
    .await;
    assert_eq!(second.status(), StatusCode::SEE_OTHER);
    assert!(
        second.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=300")
    );
    sqlx::query(
        "DELETE FROM uac_role_acl WHERE role_id='analytics-admin'
        AND acl_id='analytics-session-update'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let denied = format!("idle_seconds=600&csrf={}", csrf(&cookie));
    let no = send(
        app(pool.clone()),
        "/settings/session",
        Some(&denied),
        Some(&cookie),
    )
    .await;
    assert_eq!(no.status(), StatusCode::FORBIDDEN);
    assert!(!no.headers().contains_key(header::SET_COOKIE));
    let unchanged: i64 =
        sqlx::query_scalar("SELECT idle_seconds FROM analytics_session_policy WHERE id=1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unchanged, 300);
}
