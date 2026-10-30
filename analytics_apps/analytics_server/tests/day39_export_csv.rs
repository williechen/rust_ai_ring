#![cfg(feature = "postgres-integration")]
mod support;

use analytics_server::bootstrap;
use axum::http::{StatusCode, header};
use support::http::{body_text, get, post_form, response_cookie};

#[sqlx::test(migrations = "../migrations")]
async fn day39_authenticated_admin_exports_only_selected_source_csv(pool: sqlx::PgPool) {
    bootstrap::initialize(&pool, "first-password-39".into())
        .await
        .unwrap();
    for (source, kind) in [
        ("web_chat", "chat.message.persisted"),
        ("web_chat", "chat.room.created"),
        ("desktop_pet", "pet.sleep.started"),
    ] {
        sqlx::query("INSERT INTO product_events(source_app,event_id,schema_version,event_type) VALUES($1,$2,1,$3)")
            .bind(source).bind(uuid::Uuid::new_v4().to_string()).bind(kind)
            .execute(&pool).await.unwrap();
    }
    let app = support::app(pool.clone(), "fixture-chat", "fixture-pet");
    let url = "/api/v1/events/web_chat/export.csv";
    let anonymous = get(app.clone(), url, None).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let machine = get(app.clone(), url, Some("analytics_session=fixture-chat")).await;
    assert_eq!(machine.status(), StatusCode::UNAUTHORIZED);

    let login = post_form(
        app.clone(),
        "/login",
        "username=admin&password=first-password-39".into(),
        None,
    )
    .await;
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
    let first = response_cookie(&login);
    assert_eq!(
        get(app.clone(), url, Some(&first)).await.status(),
        StatusCode::FORBIDDEN
    );
    let change = get(app.clone(), "/change-password", Some(&first)).await;
    let csrf = body_text(change)
        .await
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .to_owned();
    let changed = post_form(app.clone(), "/change-password",
        format!("current_password=first-password-39&new_password=next-password-39&confirm_password=next-password-39&email=day39%40example.test&csrf={csrf}"),
        Some(&first)).await;
    assert_eq!(changed.status(), StatusCode::SEE_OTHER);
    let login = post_form(
        app.clone(),
        "/login",
        "username=admin&password=next-password-39".into(),
        None,
    )
    .await;
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
    let cookie = response_cookie(&login);
    let page = get(app.clone(), "/dashboard?source=web_chat", Some(&cookie)).await;
    assert_eq!(page.status(), StatusCode::OK);
    assert!(
        body_text(page)
            .await
            .contains("/api/v1/events/web_chat/export.csv")
    );

    let csv = get(app.clone(), url, Some(&cookie)).await;
    assert_eq!(csv.status(), StatusCode::OK);
    assert_eq!(
        csv.headers()[header::CONTENT_TYPE],
        "text/csv; charset=utf-8"
    );
    assert_eq!(
        csv.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"analytics-web_chat-summary.csv\""
    );
    let body = body_text(csv).await;
    assert!(body.contains("chat.message.persisted"));
    assert!(body.contains("chat.room.created"));
    assert!(!body.contains("pet.sleep.started"));
    let pet = get(
        app.clone(),
        "/api/v1/events/desktop_pet/export.csv",
        Some(&cookie),
    )
    .await;
    assert_eq!(pet.status(), StatusCode::OK);
    let body = body_text(pet).await;
    assert!(body.contains("pet.sleep.started"));
    assert!(!body.contains("chat.message.persisted"));
    assert_eq!(
        get(
            app.clone(),
            "/api/v1/events/unknown/export.csv",
            Some(&cookie)
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );

    let filtered = get(
        app.clone(),
        "/api/v1/events/web_chat/export.csv?event_type=chat.message.persisted",
        Some(&cookie),
    )
    .await;
    assert_eq!(filtered.status(), StatusCode::OK);
    let body = body_text(filtered).await;
    assert!(body.contains("chat.message.persisted"));
    assert!(!body.contains("chat.room.created"));
    assert!(!body.contains("pet.sleep.started"));
    let no_kind = get(
        app.clone(),
        "/api/v1/events/web_chat/export.csv?event_type=missing.kind",
        Some(&cookie),
    )
    .await;
    assert_eq!(no_kind.status(), StatusCode::OK);
    assert!(!body_text(no_kind).await.contains("chat.message.persisted"));
    let invalid = get(
        app.clone(),
        "/api/v1/events/web_chat/export.csv?event_type=%20invalid",
        Some(&cookie),
    )
    .await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    let page = get(
        app.clone(),
        "/dashboard?source=web_chat&event_type=chat.message.persisted",
        Some(&cookie),
    )
    .await;
    assert_eq!(page.status(), StatusCode::OK);
    let html = body_text(page).await;
    assert!(html.contains("/api/v1/events/web_chat/export.csv?event_type=chat.message.persisted"));
    let csrf = html
        .split("action=\"/logout\"")
        .nth(1)
        .unwrap()
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();

    let logout = post_form(
        app.clone(),
        "/logout",
        format!("csrf={csrf}"),
        Some(&cookie),
    )
    .await;
    assert_eq!(logout.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        get(app, url, Some(&cookie)).await.status(),
        StatusCode::UNAUTHORIZED
    );
}
