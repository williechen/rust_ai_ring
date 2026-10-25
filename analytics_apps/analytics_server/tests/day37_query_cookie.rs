#![cfg(feature = "postgres-integration")]
use analytics_server::{IngestState, IngestTokens, bootstrap, password_login};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use std::sync::Arc;
use tower::ServiceExt;

async fn query(
    app: &Router,
    path: &str,
    cookies: &[String],
    bearer: bool,
) -> axum::response::Response {
    let mut request = Request::get(path).body(Body::empty()).unwrap();
    for cookie in cookies {
        request
            .headers_mut()
            .append(header::COOKIE, cookie.parse().unwrap());
    }
    if bearer {
        request.headers_mut().insert(
            header::AUTHORIZATION,
            "Bearer fixture-chat".parse().unwrap(),
        );
    }
    app.clone().oneshot(request).await.unwrap()
}

#[sqlx::test(migrations = "../migrations")]
async fn day37_real_query_routes_reject_ambiguous_session_cookies(pool: sqlx::PgPool) {
    bootstrap::initialize(&pool, "day37-first-password".into())
        .await
        .unwrap();
    // Fixture 狀態：已完成首次改密碼；不把直接 SQL 當 UI 流程驗收。
    sqlx::query("UPDATE uac_user SET is_init_pd=false WHERE lower(user_acnt)='admin'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO product_events(source_app,event_id,schema_version,event_type)
        VALUES('web_chat',$1,1,'chat.message.persisted')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .execute(&pool)
    .await
    .unwrap();
    let app = password_login::build(IngestState {
        pool: pool.clone(),
        tokens: Arc::new(IngestTokens {
            chat: "fixture-chat".into(),
            pet: "fixture-pet".into(),
        }),
    });
    let login = app
        .clone()
        .oneshot(
            Request::post("/login")
                .header(header::ORIGIN, "http://127.0.0.1:3100")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=admin&password=day37-first-password"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
    assert_eq!(login.headers()[header::LOCATION], "/dashboard");
    let cookie = login.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let invalid = format!("analytics_session={}", "0".repeat(64));
    let paths = [
        "/api/v1/events/web_chat/summary",
        "/api/v1/events/web_chat/summary/chat.message.persisted",
    ];
    for path in paths {
        assert_eq!(
            query(&app, path, &[], false).await.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            query(&app, path, &[], true).await.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            query(&app, path, &[cookie.clone()], false).await.status(),
            StatusCode::OK
        );
        for ambiguous in [
            vec![format!("{cookie}; {invalid}")],
            vec![cookie.clone(), invalid.clone()],
            vec![invalid.clone(), cookie.clone()],
            vec![format!("{cookie}; {cookie}")],
            vec![format!("{cookie}; analytics_session")],
        ] {
            let response = query(&app, path, &ambiguous, false).await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
            assert!(!response.headers().contains_key(header::SET_COOKIE));
        }
        // 不同 cookie 名稱仍合法；拒絕歧義不應撤銷原有效 session。
        assert_eq!(
            query(&app, path, &[format!("theme=dark; {cookie}")], false)
                .await
                .status(),
            StatusCode::OK
        );
    }
    sqlx::query("UPDATE uac_user SET is_init_pd=true WHERE lower(user_acnt)='admin'")
        .execute(&pool)
        .await
        .unwrap();
    for path in paths {
        assert_eq!(
            query(&app, path, &[cookie.clone()], false).await.status(),
            StatusCode::FORBIDDEN
        );
    }
    sqlx::query("UPDATE uac_user SET is_init_pd=false WHERE lower(user_acnt)='admin'")
        .execute(&pool)
        .await
        .unwrap();
    let logout = app
        .clone()
        .oneshot(
            Request::post("/logout")
                .header(header::ORIGIN, "http://127.0.0.1:3100")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::SEE_OTHER);
    for path in paths {
        assert_eq!(
            query(&app, path, &[cookie.clone()], false).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }
}
