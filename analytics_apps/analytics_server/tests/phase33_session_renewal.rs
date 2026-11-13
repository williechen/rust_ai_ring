#![cfg(feature = "postgres-integration")]
mod support;
use axum::http::{StatusCode, header};
use shared::auth::token_digest;
use sqlx::PgPool;
use support::http::send;

const RAW: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

async fn fixture(pool: &PgPool) -> (String, String, i64) {
    let user = analytics_server::bootstrap::initialize(pool, "phase33-test-password".into())
        .await
        .unwrap();
    sqlx::query("UPDATE uac_user SET is_init_pd=FALSE")
        .execute(pool)
        .await
        .unwrap();
    let encoded: String = sqlx::query_scalar("SELECT user_pd FROM uac_user WHERE id=$1")
        .bind(&user)
        .fetch_one(pool)
        .await
        .unwrap();
    let expiry: i64 = sqlx::query_scalar(
        "INSERT INTO uac_login_session(token_hash,user_id,credential_fingerprint,restricted,expires_at)
         VALUES($1,$2,$3,FALSE,floor(extract(epoch from now()))::bigint+60) RETURNING expires_at")
        .bind(token_digest(RAW)).bind(&user).bind(token_digest(&encoded))
        .fetch_one(pool).await.unwrap();
    (user, encoded, expiry)
}

async fn expiry(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT expires_at FROM uac_login_session WHERE token_hash=$1")
        .bind(token_digest(RAW))
        .fetch_one(pool)
        .await
        .unwrap()
}

fn cookie() -> String {
    format!("analytics_session={RAW}")
}

#[sqlx::test(migrations = "../migrations")]
async fn rejected_csrf_preserves_database_expiry_and_cookie(pool: PgPool) {
    let (_, _, before) = fixture(&pool).await;
    let response = send(
        support::app(pool.clone(), "chat", "pet"),
        "/settings/session",
        Some("idle_seconds=300&csrf=wrong"),
        Some(&cookie()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!response.headers().contains_key(header::SET_COOKIE));
    assert_eq!(expiry(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn denied_page_preserves_database_expiry_and_cookie(pool: PgPool) {
    let (_, _, before) = fixture(&pool).await;
    sqlx::query("DELETE FROM uac_role_acl WHERE role_id='analytics-admin' AND acl_id='analytics-dashboard-read'")
        .execute(&pool).await.unwrap();
    let response = send(
        support::app(pool.clone(), "chat", "pet"),
        "/dashboard",
        None,
        Some(&cookie()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!response.headers().contains_key(header::SET_COOKIE));
    assert_eq!(expiry(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn invalid_page_preserves_database_expiry_and_cookie(pool: PgPool) {
    let (_, _, before) = fixture(&pool).await;
    let response = send(
        support::app(pool.clone(), "chat", "pet"),
        "/dashboard?page=0",
        None,
        Some(&cookie()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(!response.headers().contains_key(header::SET_COOKIE));
    assert_eq!(expiry(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn query_failure_preserves_database_expiry_and_cookie(pool: PgPool) {
    let (_, _, before) = fixture(&pool).await;
    sqlx::query("ALTER TABLE product_events RENAME TO unavailable_events")
        .execute(&pool)
        .await
        .unwrap();
    let response = send(
        support::app(pool.clone(), "chat", "pet"),
        "/dashboard",
        None,
        Some(&cookie()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(!response.headers().contains_key(header::SET_COOKIE));
    assert_eq!(expiry(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn successful_page_renews_once_with_matching_database_and_cookie_ttl(pool: PgPool) {
    let (_, _, before) = fixture(&pool).await;
    sqlx::raw_sql("CREATE TABLE renewal_probe (id integer);
        CREATE FUNCTION count_renewal() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO renewal_probe VALUES(1); RETURN NEW; END $$;
        CREATE TRIGGER count_renewal AFTER UPDATE ON uac_login_session FOR EACH ROW EXECUTE FUNCTION count_renewal();")
        .execute(&pool).await.unwrap();
    let response = send(
        support::app(pool.clone(), "chat", "pet"),
        "/dashboard",
        None,
        Some(&cookie()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=1800")
    );
    let remaining: i64 = sqlx::query_scalar("SELECT expires_at-floor(extract(epoch from now()))::bigint FROM uac_login_session WHERE token_hash=$1")
        .bind(token_digest(RAW)).fetch_one(&pool).await.unwrap();
    assert!((1790..=1800).contains(&remaining));
    assert!(expiry(&pool).await > before);
    let updates: i64 = sqlx::query_scalar("SELECT count(*) FROM renewal_probe")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(updates, 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn adapter_cannot_renew_another_users_session(pool: PgPool) {
    let (_, encoded, before) = fixture(&pool).await;
    sqlx::query("INSERT INTO uac_user(id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at)
        VALUES('other','other',$1,'other@example.test','other','active',FALSE,now(),now())")
        .bind(&encoded).execute(&pool).await.unwrap();
    let outcome =
        analytics_persistence::login_session::refresh(&pool, &token_digest(RAW), "other", &encoded)
            .await
            .unwrap();
    assert!(outcome.is_none());
    assert_eq!(expiry(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn adapter_cannot_renew_a_stale_fingerprint(pool: PgPool) {
    let (user, encoded, before) = fixture(&pool).await;
    sqlx::query("UPDATE uac_login_session SET credential_fingerprint=$1 WHERE token_hash=$2")
        .bind("f".repeat(64))
        .bind(token_digest(RAW))
        .execute(&pool)
        .await
        .unwrap();
    let outcome =
        analytics_persistence::login_session::refresh(&pool, &token_digest(RAW), &user, &encoded)
            .await
            .unwrap();
    assert!(outcome.is_none());
    assert_eq!(expiry(&pool).await, before);
}
