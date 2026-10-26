#![cfg(feature = "postgres-integration")]
mod support;

use axum::http::{StatusCode, header};
use support::http::{response_cookie as cookie, send};

fn app(pool: sqlx::PgPool) -> axum::Router {
    support::app(pool, "fixture-chat", "fixture-pet")
}

#[sqlx::test(migrations = "../migrations")]
async fn day23_bootstrapped_password_is_restricted_before_first_change(pool: sqlx::PgPool) {
    analytics_server::bootstrap::initialize(&pool, "lesson-only-password".into())
        .await
        .unwrap();
    assert_eq!(
        send(
            app(pool.clone()),
            "/login",
            Some("username=admin&password=wrong"),
            None
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM uac_login_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let ok = send(
        app(pool.clone()),
        "/login",
        Some("username=ADMIN&password=lesson-only-password"),
        None,
    )
    .await;
    assert_eq!(ok.status(), StatusCode::SEE_OTHER);
    assert_eq!(ok.headers()[header::LOCATION], "/change-password");
    let c = cookie(&ok);
    assert_eq!(
        send(app(pool.clone()), "/dashboard", None, Some(&c))
            .await
            .headers()[header::LOCATION],
        "/change-password"
    );
    assert_eq!(
        send(
            app(pool.clone()),
            "/api/v1/events/web_chat/summary",
            None,
            Some(&c)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let pending = send(app(pool.clone()), "/change-password", None, Some(&c)).await;
    assert_eq!(pending.status(), StatusCode::OK);
    let body = axum::body::to_bytes(pending.into_body(), 100000)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("儲存並重新登入"));
    assert!(
        analytics_server::bootstrap::initialize(&pool, "different-password".into())
            .await
            .is_err()
    );
    let flag: bool = sqlx::query_scalar("SELECT is_init_pd FROM uac_user")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(flag);
}
#[sqlx::test(migrations = "../migrations")]
async fn day23_ready_account_session_survives_new_pool_and_is_revocable(pool: sqlx::PgPool) {
    analytics_server::bootstrap::initialize(&pool, "lesson-only-password".into())
        .await
        .unwrap();
    // 已完成首次密碼更換的既有帳號fixture；此SQL不是提供給使用者繞過首次改密碼的操作。
    sqlx::query("UPDATE uac_user SET is_init_pd=FALSE")
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
    assert_eq!(ok.headers()[header::LOCATION], "/dashboard");
    let set = ok.headers()[header::SET_COOKIE].to_str().unwrap();
    for attr in ["HttpOnly", "SameSite=Strict", "Max-Age=1800"] {
        assert!(set.contains(attr));
    }
    let c = cookie(&ok);
    let raw = c.split_once('=').unwrap().1;
    let stored: String = sqlx::query_scalar("SELECT token_hash FROM uac_login_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(stored, raw);
    assert_eq!(stored.len(), 64);
    let options = pool.connect_options();
    pool.close().await;
    let reopened = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_with((*options).clone())
        .await
        .unwrap();
    let ok = send(app(reopened.clone()), "/dashboard", None, Some(&c)).await;
    assert_eq!(ok.status(), StatusCode::OK);
    assert!(ok.headers().contains_key(header::SET_COOKIE));
    assert_eq!(
        send(
            app(reopened.clone()),
            "/api/v1/events/web_chat/summary",
            None,
            Some(&c)
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            app(reopened.clone()),
            "/api/v1/events/web_chat/summary",
            None,
            None
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    sqlx::query("UPDATE uac_user SET user_status='disabled'")
        .execute(&reopened)
        .await
        .unwrap();
    assert_eq!(
        send(
            app(reopened.clone()),
            "/api/v1/events/web_chat/summary",
            None,
            Some(&c)
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    sqlx::query("UPDATE uac_user SET user_status='active'")
        .execute(&reopened)
        .await
        .unwrap();
    sqlx::query("UPDATE uac_login_session SET expires_at=0")
        .execute(&reopened)
        .await
        .unwrap();
    assert_eq!(
        send(
            app(reopened),
            "/api/v1/events/web_chat/summary",
            None,
            Some(&c)
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
}
