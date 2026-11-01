#![cfg(feature = "postgres-integration")]
mod support;
use analytics_server::{IngestState, IngestTokens, bootstrap, password_login};
use axum::http::{StatusCode, header};
use std::sync::Arc;
use support::http::{body_text, response_cookie, send};

fn csrf(html: &str, action: &str) -> String {
    html.split(&format!("action=\"{action}\""))
        .nth(1)
        .unwrap()
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .into()
}

#[sqlx::test(migrations = "../migrations")]
async fn disabling_viewer_revokes_live_sessions_and_writes_audit(pool: sqlx::PgPool) {
    let admin = bootstrap::initialize(&pool, "lesson-password".into())
        .await
        .unwrap();
    sqlx::query("UPDATE uac_user SET is_init_pd=FALSE WHERE id=$1")
        .bind(&admin)
        .execute(&pool)
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
        Some("username=admin&password=lesson-password"),
        None,
    )
    .await;
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
    let admin_cookie = response_cookie(&login);
    let page = body_text(send(app.clone(), "/accounts/new", None, Some(&admin_cookie)).await).await;
    let create_token = csrf(&page, "/accounts");
    let form = format!(
        "username=reader46&password=reader-password&email=reader46%40example.test&chat=on&csrf={create_token}"
    );
    assert_eq!(
        send(app.clone(), "/accounts", Some(&form), Some(&admin_cookie))
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let id: String = sqlx::query_scalar("SELECT id FROM uac_user WHERE user_acnt='reader46'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let reader_login = send(
        app.clone(),
        "/login",
        Some("username=reader46&password=reader-password"),
        None,
    )
    .await;
    assert_eq!(reader_login.status(), StatusCode::SEE_OTHER);
    let reader_cookie = response_cookie(&reader_login);
    let action = format!("/accounts/{id}/disable");
    let page = body_text(send(app.clone(), "/accounts/new", None, Some(&admin_cookie)).await).await;
    let token = csrf(&page, &action);
    assert_eq!(
        send(
            app.clone(),
            &action,
            Some("csrf=wrong"),
            Some(&admin_cookie)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let status: String = sqlx::query_scalar("SELECT user_status FROM uac_user WHERE id=$1")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "active");
    let response = send(
        app.clone(),
        &action,
        Some(&format!("csrf={token}")),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.headers()[header::LOCATION],
        "/accounts/new?disabled=1"
    );
    let status: String = sqlx::query_scalar("SELECT user_status FROM uac_user WHERE id=$1")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "disabled");
    let sessions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM uac_login_session WHERE user_id=$1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sessions, 0);
    assert_eq!(
        send(app.clone(), "/dashboard", None, Some(&reader_cookie))
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        send(
            app.clone(),
            "/login",
            Some("username=reader46&password=reader-password"),
            None
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM analytics_account_audit WHERE actor_id=$1 AND target_id=$2 AND action='disable_viewer' AND result='disabled'").bind(&admin).bind(&id).fetch_one(&pool).await.unwrap();
    assert_eq!(audits, 1);
    assert_eq!(
        send(app, "/dashboard", None, Some(&admin_cookie))
            .await
            .status(),
        StatusCode::OK
    );
}
