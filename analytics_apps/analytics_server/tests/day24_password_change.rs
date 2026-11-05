#![cfg(feature = "postgres-integration")]

use analytics_server::{IngestState, IngestTokens, password_login};
use axum::{
    body::Body,
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

async fn send(
    app: axum::Router,
    path: &str,
    body: Option<&str>,
    cookie: Option<&str>,
) -> axum::response::Response {
    let mut r = if body.is_some() {
        Request::post(path)
            .header("origin", "http://127.0.0.1:3100")
            .header("content-type", "application/x-www-form-urlencoded")
    } else {
        Request::get(path)
    };
    if let Some(c) = cookie {
        r = r.header(header::COOKIE, c);
    }
    app.oneshot(
        r.body(body.map_or_else(Body::empty, |s| Body::from(s.to_owned())))
            .unwrap(),
    )
    .await
    .unwrap()
}

fn cookie(r: &axum::response::Response) -> String {
    r.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

#[sqlx::test(migrations = "../migrations")]
async fn day24_password_change_is_atomic_revokes_all_sessions_and_requires_relogin(
    pool: sqlx::PgPool,
) {
    analytics_server::bootstrap::initialize(&pool, "lesson-only-password".into())
        .await
        .unwrap();
    let login = send(
        app(pool.clone()),
        "/login",
        Some("username=admin&password=lesson-only-password"),
        None,
    )
    .await;
    let c = cookie(&login);
    // Before Green this is 405: POST is not implemented.
    let denied=send(app(pool.clone()),"/change-password",Some("current_password=lesson-only-password&new_password=new-lesson-password&confirm_password=new-lesson-password&email=learner%40example.test&csrf=wrong"),Some(&c)).await;
    println!("Denied response: {:?}", denied);
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let page = send(app(pool.clone()), "/change-password", None, Some(&c)).await;
    let html = String::from_utf8(
        axum::body::to_bytes(page.into_body(), 10000)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    let csrf = html
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    assert_eq!(csrf.len(), 64);
    let body = format!(
        "current_password=lesson-only-password&new_password=new-lesson-password&confirm_password=new-lesson-password&email=learner%40example.test&csrf={csrf}"
    );
    let wrong = body.replace(
        "current_password=lesson-only-password",
        "current_password=wrong-password",
    );
    assert_eq!(
        send(
            app(pool.clone()),
            "/change-password",
            Some(&wrong),
            Some(&c)
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let mismatch = body.replace(
        "confirm_password=new-lesson-password",
        "confirm_password=not-matching-password",
    );
    assert_eq!(
        send(
            app(pool.clone()),
            "/change-password",
            Some(&mismatch),
            Some(&c)
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let foreign = app(pool.clone())
        .oneshot(
            Request::post("/change-password")
                .header("origin", "https://foreign.example")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("cookie", &c)
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
    sqlx::query("INSERT INTO uac_user(id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at) VALUES('other','other','unused','learner@example.test','Other','active',FALSE,now(),now())").execute(&pool).await.unwrap();
    assert_eq!(
        send(app(pool.clone()), "/change-password", Some(&body), Some(&c))
            .await
            .status(),
        StatusCode::CONFLICT
    );
    let flag: bool =
        sqlx::query_scalar("SELECT is_init_pd FROM uac_user WHERE lower(user_acnt)='admin'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(flag);
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
    sqlx::query("DELETE FROM uac_user WHERE id='other'")
        .execute(&pool)
        .await
        .unwrap();
    let second = send(
        app(pool.clone()),
        "/login",
        Some("username=admin&password=lesson-only-password"),
        None,
    )
    .await;
    let c2 = cookie(&second);
    sqlx::query(
        "UPDATE uac_login_attempt SET failures='3',next_delay='60',blocked_until='99999999999'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let done = send(app(pool.clone()), "/change-password", Some(&body), Some(&c)).await;
    let attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM uac_login_attempt")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(attempts, 0);
    assert_eq!(done.status(), StatusCode::SEE_OTHER);
    assert_eq!(done.headers()[header::LOCATION], "/login");
    assert!(
        done.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM uac_login_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    for old in [&c, &c2] {
        assert_eq!(
            send(
                app(pool.clone()),
                "/api/v1/events/web_chat/summary",
                None,
                Some(old)
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    let row: (bool, String, String) = sqlx::query_as(
        "SELECT is_init_pd,user_email,user_pd FROM uac_user WHERE lower(user_acnt)='admin'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!row.0);
    assert_eq!(row.1, "learner@example.test");
    assert_ne!(row.2, "new-lesson-password");
    assert_eq!(
        send(
            app(pool.clone()),
            "/login",
            Some("username=admin&password=lesson-only-password"),
            None
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    // Advance only isolated fixture for old corrected-password success assertions.
    sqlx::query("UPDATE uac_login_attempt SET blocked_until='0'")
        .execute(&pool)
        .await
        .unwrap();
    let login = send(
        app(pool.clone()),
        "/login",
        Some("username=admin&password=new-lesson-password"),
        None,
    )
    .await;
    assert_eq!(login.headers()[header::LOCATION], "/dashboard");
    assert_eq!(
        send(app(pool.clone()), "/dashboard", None, Some(&cookie(&login)))
            .await
            .status(),
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn day24_duplicate_migration_stops_and_revocation_failure_rolls_back(pool: sqlx::PgPool) {
    analytics_server::bootstrap::initialize(&pool, "lesson-only-password".into())
        .await
        .unwrap();
    sqlx::query("DROP INDEX uq_uac_user_email")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO uac_user(id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at) VALUES('duplicate','other','unused',' ADMIN@LOCALHOST.INVALID ','Other','active',FALSE,now(),now())").execute(&pool).await.unwrap();
    assert!(
        sqlx::raw_sql(include_str!(
            "../../migrations/20261021071000_unique_account_email.sql"
        ))
        .execute(&pool)
        .await
        .is_err()
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM uac_user")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2); // No deletion to force a migration pass.
    sqlx::query("DELETE FROM uac_user WHERE id='duplicate'")
        .execute(&pool)
        .await
        .unwrap(); // Fixture-only manual resolution.
    sqlx::raw_sql(include_str!(
        "../../migrations/20261021071000_unique_account_email.sql"
    ))
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
    let c = cookie(&login);
    let page = send(app(pool.clone()), "/change-password", None, Some(&c)).await;
    let html = String::from_utf8(
        axum::body::to_bytes(page.into_body(), 10000)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    let csrf = html
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let before: String = sqlx::query_scalar("SELECT user_pd FROM uac_user")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION fixture_fail_revoke() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture'; END; $$; CREATE TRIGGER fixture_fail_revoke BEFORE DELETE ON uac_login_session FOR EACH ROW EXECUTE FUNCTION fixture_fail_revoke();").execute(&pool).await.unwrap();
    let body = format!(
        "current_password=lesson-only-password&new_password=new-lesson-password&confirm_password=new-lesson-password&email=learner%40example.test&csrf={csrf}"
    );
    assert_eq!(
        send(app(pool.clone()), "/change-password", Some(&body), Some(&c))
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let row: (String, bool, String) =
        sqlx::query_as("SELECT user_pd,is_init_pd,user_email FROM uac_user")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row, (before, true, "admin@localhost.invalid".into()));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM uac_login_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
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

    #[sqlx::test(migrations = "../migrations")]
    async fn day31_change_page_has_connected_labels_and_existing_csrf(pool: sqlx::PgPool) {
        analytics_server::bootstrap::initialize(&pool, "lesson-only-password".into())
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
        let c = cookie(&login);
        let page = send(app(pool.clone()), "/change-password", None, Some(&c)).await;
        assert_eq!(page.status(), StatusCode::OK);
        assert!(page.headers().contains_key(header::SET_COOKIE));
        let bytes = axum::body::to_bytes(page.into_body(), 16000).await.unwrap();
        let html = String::from_utf8(bytes.to_vec()).unwrap();
        assert_eq!(html.matches("<!doctype html>").count(), 1);
        assert!(html.contains("for=\"change-current\""));
        assert!(html.contains("id=\"change-current\""));
        assert!(html.contains("action=\"/change-password\""));
        assert!(html.contains("首次登入須更換密碼與Email"));
        let csrf = html
            .split("name=\"csrf\" value=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        assert_eq!(csrf.len(), 64);
        assert!(csrf.bytes().all(|b| b.is_ascii_hexdigit()));
        // 沒有 Cookie 的同一個正式 router，仍不可取得表單。
        assert_eq!(
            send(app(pool), "/change-password", None, None)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
}
