#![cfg(feature = "postgres-integration")]

mod support;

use analytics_server::bootstrap;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use support::http::{body_text, form_csrf, get, post_form, response_cookie};
use tower::ServiceExt;

const CHAT: &str = "fixture-chat";
const PET: &str = "fixture-pet";
const CHAT_EVENT: &str = "chat.message.persisted";
const PET_EVENT: &str = "pet.sleep";
const CHAT_SUMMARY: &str = "/api/v1/events/web_chat/summary";
const PET_SUMMARY: &str = "/api/v1/events/desktop_pet/summary";

async fn login(app: &Router, user: &str, password: &str, landing: &str) -> String {
    let response = post_form(
        app.clone(),
        "/login",
        format!("username={user}&password={password}"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], landing);
    response_cookie(&response)
}

async fn change_first_password(app: &Router, first: &str, current: &str, new: &str, email: &str) {
    assert_eq!(
        get(app.clone(), CHAT_SUMMARY, Some(first)).await.status(),
        StatusCode::FORBIDDEN
    );
    let page = get(app.clone(), "/change-password", Some(first)).await;
    assert_eq!(page.status(), StatusCode::OK);
    let csrf = form_csrf(&body_text(page).await, "/change-password");
    let response = post_form(app.clone(), "/change-password",
        format!("current_password={current}&new_password={new}&confirm_password={new}&email={email}&csrf={csrf}"),
        Some(first)).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], "/login");
    assert_eq!(
        get(app.clone(), CHAT_SUMMARY, Some(first)).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

async fn ingest(
    app: &Router,
    token: Option<&str>,
    cookie: Option<&str>,
    source: &str,
    id: uuid::Uuid,
    kind: &str,
) -> StatusCode {
    let payload = serde_json::json!({
        "schema_version": 1, "event_id": id, "source_app": source, "event_type": kind
    });
    let mut request =
        Request::post("/api/v1/events").header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.clone()
        .oneshot(request.body(Body::from(payload.to_string())).unwrap())
        .await
        .unwrap()
        .status()
}

fn picker(html: &str) -> &str {
    html.split_once("<select")
        .expect("來源分析須有共用來源下拉選單")
        .1
        .split_once("</select>")
        .unwrap()
        .0
}

fn assert_options(html: &str, chat: bool, pet: bool) {
    let control = picker(html);
    assert!(control.contains("name=\"source\""));
    assert_eq!(control.contains("value=\"web_chat\""), chat);
    assert_eq!(control.contains("value=\"desktop_pet\""), pet);
    assert_eq!(html.matches(">來源分析</a>").count(), 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn first_login_source_picker_and_same_session_scope_follow_admin_changes(pool: sqlx::PgPool) {
    let admin_id = bootstrap::ensure_default(&pool).await.unwrap();
    let app = support::app(pool.clone(), CHAT, PET);
    let first = login(&app, "admin", "admin", "/change-password").await;
    change_first_password(
        &app,
        &first,
        "admin",
        "day55-admin-password",
        "admin%40example.test",
    )
    .await;
    let admin = login(&app, "admin", "day55-admin-password", "/dashboard").await;
    assert_eq!(
        get(
            app.clone(),
            CHAT_SUMMARY,
            Some("analytics_session=fixture-chat")
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let id = uuid::Uuid::new_v4();
    assert_eq!(
        ingest(&app, None, Some(&admin), "web_chat", id, CHAT_EVENT).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        ingest(&app, Some(PET), None, "web_chat", id, CHAT_EVENT).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        ingest(&app, Some(CHAT), None, "web_chat", id, CHAT_EVENT).await,
        StatusCode::CREATED
    );
    assert_eq!(
        ingest(&app, Some(CHAT), None, "web_chat", id, CHAT_EVENT).await,
        StatusCode::OK
    );
    assert_eq!(
        ingest(&app, Some(CHAT), None, "web_chat", id, "conflict").await,
        StatusCode::CONFLICT
    );
    assert_eq!(
        ingest(&app, Some(PET), None, "desktop_pet", id, PET_EVENT).await,
        StatusCode::CREATED
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM product_events")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    let admin_dashboard = get(app.clone(), "/dashboard", Some(&admin)).await;
    assert_eq!(admin_dashboard.status(), StatusCode::OK);
    assert_options(&body_text(admin_dashboard).await, true, true);

    let page = body_text(get(app.clone(), "/accounts/new", Some(&admin)).await).await;
    let csrf = form_csrf(&page, "/accounts");
    let created = post_form(app.clone(), "/accounts",
        format!("username=viewer55&password=viewer-first-password&email=viewer55%40example.test&chat=on&csrf={csrf}"),
        Some(&admin)).await;
    assert_eq!(created.status(), StatusCode::SEE_OTHER);
    let first = login(
        &app,
        "viewer55",
        "viewer-first-password",
        "/change-password",
    )
    .await;
    change_first_password(
        &app,
        &first,
        "viewer-first-password",
        "viewer-next-password",
        "viewer55%40example.test",
    )
    .await;
    let viewer = login(&app, "viewer55", "viewer-next-password", "/dashboard").await;

    let report = get(
        app.clone(),
        "/dashboard?source=web_chat&event_type=chat.message.persisted",
        Some(&viewer),
    )
    .await;
    assert_eq!(report.status(), StatusCode::OK);
    let html = body_text(report).await;
    assert_options(&html, true, false);
    assert!(html.contains("1 筆已提交事件"));
    assert!(html.contains("/api/v1/events/web_chat/export.csv?event_type=chat.message.persisted"));
    for path in [
        "/accounts/new",
        "/dashboard?source=desktop_pet",
        PET_SUMMARY,
        "/api/v1/events/desktop_pet/export.csv",
    ] {
        assert_eq!(
            get(app.clone(), path, Some(&viewer)).await.status(),
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }

    let viewer_id: String =
        sqlx::query_scalar("SELECT id FROM uac_user WHERE user_acnt='viewer55'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let action = format!("/accounts/{viewer_id}/sources");
    let page = body_text(get(app.clone(), "/accounts/new", Some(&admin)).await).await;
    let csrf = form_csrf(&page, &action);
    let changed = post_form(
        app.clone(),
        &action,
        format!("chat=deny&pet=allow&csrf={csrf}"),
        Some(&admin),
    )
    .await;
    assert_eq!(changed.status(), StatusCode::SEE_OTHER);
    let audit: (String, String, String) = sqlx::query_as(
        "SELECT actor_id,target_id,result FROM analytics_account_audit WHERE action='update_viewer_sources'",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(
        audit,
        (
            admin_id.clone(),
            viewer_id.clone(),
            "chat=deny;pet=allow".into()
        )
    );
    for path in [
        "/dashboard?source=web_chat",
        CHAT_SUMMARY,
        "/api/v1/events/web_chat/export.csv",
    ] {
        assert_eq!(
            get(app.clone(), path, Some(&viewer)).await.status(),
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
    let hash: String = sqlx::query_scalar("SELECT user_pd FROM uac_user WHERE id=$1")
        .bind(&admin_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(hash.starts_with("$argon2id$"));
    assert_eq!(bootstrap::ensure_default(&pool).await.unwrap(), admin_id);
    let hash_after: String = sqlx::query_scalar("SELECT user_pd FROM uac_user WHERE id=$1")
        .bind(&admin_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(hash, hash_after);
    let reopened = support::app(pool.clone(), CHAT, PET);
    let report = get(
        reopened.clone(),
        "/dashboard?event_type=pet.sleep",
        Some(&viewer),
    )
    .await;
    assert_eq!(report.status(), StatusCode::OK);
    let html = body_text(report).await;
    assert_options(&html, false, true);
    assert!(html.contains("1 筆已提交事件"));
    let csrf = form_csrf(&html, "/logout");
    assert_eq!(
        post_form(
            reopened.clone(),
            "/logout",
            format!("csrf={csrf}"),
            Some(&viewer)
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        get(reopened, PET_SUMMARY, Some(&viewer)).await.status(),
        StatusCode::UNAUTHORIZED
    );
    eprintln!("首次改密碼、來源切換、已提交 2 列、跨來源拒絕、權限稽核及登出已驗證");
}
