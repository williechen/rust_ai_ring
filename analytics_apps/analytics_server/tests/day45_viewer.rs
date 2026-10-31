#![cfg(feature = "postgres-integration")]
mod support;
use analytics_server::{IngestState, IngestTokens, bootstrap, password_login};
use axum::http::{StatusCode, header};
use std::sync::Arc;
use support::http::{body_text, response_cookie, send};

fn token(html: &str, action: &str) -> String {
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
async fn admin_creates_viewer_first_change_and_source_revoke_affects_same_session(
    pool: sqlx::PgPool,
) {
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
    let cookie = response_cookie(&login);
    // ACL classification does not manufacture an unimplemented DELETE route.
    {
        use tower::ServiceExt;
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::delete("/accounts")
                    .header(header::COOKIE, &cookie)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    let page = send(app.clone(), "/accounts/new", None, Some(&cookie)).await;
    assert_eq!(page.status(), StatusCode::OK);
    let csrf = token(&body_text(page).await, "/accounts");
    let form = format!(
        "username=reader&password=reader-password&email=reader%40example.test&chat=on&csrf={csrf}"
    );
    assert_eq!(
        send(
            app.clone(),
            "/accounts",
            Some(&form.replace(&csrf, "wrong")),
            Some(&cookie)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let created = send(app.clone(), "/accounts", Some(&form), Some(&cookie)).await;
    assert_eq!(created.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        created.headers()[header::LOCATION],
        "/accounts/new?created=1"
    );
    let encoded: String =
        sqlx::query_scalar("SELECT user_pd FROM uac_user WHERE user_acnt='reader'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(encoded.starts_with("$argon2"));
    assert!(!encoded.contains("reader-password"));
    assert_eq!(
        send(app.clone(), "/accounts", Some(&form), Some(&cookie))
            .await
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(
            app.clone(),
            "/accounts",
            Some(&form.replace("username=reader", "username=reader2")),
            Some(&cookie)
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    let orphan: i64 = sqlx::query_scalar("SELECT count(*) FROM uac_user WHERE user_acnt='reader2'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(orphan, 0);
    let login = send(
        app.clone(),
        "/login",
        Some("username=reader&password=reader-password"),
        None,
    )
    .await;
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
    assert_eq!(login.headers()[header::LOCATION], "/change-password");
    let reader = response_cookie(&login);
    assert_eq!(
        send(
            app.clone(),
            "/api/v1/events/web_chat/summary",
            None,
            Some(&reader)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let page = body_text(send(app.clone(), "/change-password", None, Some(&reader)).await).await;
    let csrf = token(&page, "/change-password");
    let changed=send(app.clone(),"/change-password",Some(&format!("current_password=reader-password&new_password=changed-password&confirm_password=changed-password&email=reader%40example.test&csrf={csrf}")),Some(&reader)).await;
    assert_eq!(changed.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        send(app.clone(), "/dashboard", None, Some(&reader))
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let reader = response_cookie(
        &send(
            app.clone(),
            "/login",
            Some("username=reader&password=changed-password"),
            None,
        )
        .await,
    );
    for path in [
        "/dashboard?source=web_chat",
        "/api/v1/events/web_chat/summary",
        "/api/v1/events/web_chat/summary/chat.message.persisted",
        "/api/v1/events/web_chat/export.csv",
    ] {
        assert_eq!(
            send(app.clone(), path, None, Some(&reader)).await.status(),
            StatusCode::OK,
            "{path}"
        );
    }
    for path in [
        "/dashboard?source=desktop_pet",
        "/api/v1/events/desktop_pet/summary",
        "/api/v1/events/desktop_pet/summary/pet.sleep",
        "/api/v1/events/desktop_pet/export.csv",
        "/accounts/new",
    ] {
        assert_eq!(
            send(app.clone(), path, None, Some(&reader)).await.status(),
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
    let csrf = token(
        &body_text(send(app.clone(), "/change-password", None, Some(&reader)).await).await,
        "/logout",
    );
    assert_eq!(
        send(
            app.clone(),
            "/accounts",
            Some(&format!(
                "username=evil&password=evil-password&email=evil%40example.test&chat=on&csrf={csrf}"
            )),
            Some(&reader)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query(
        "DELETE FROM uac_user_acl WHERE user_id=(SELECT id FROM uac_user WHERE user_acnt='reader')",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        send(
            app.clone(),
            "/api/v1/events/web_chat/export.csv",
            None,
            Some(&reader)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    // A Pet-only viewer can enter the default dashboard after a fresh login too.
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) SELECT id,'analytics-pet','allow',now() FROM uac_user WHERE user_acnt='reader'")
        .execute(&pool).await.unwrap();
    let page = send(app.clone(), "/dashboard", None, Some(&reader)).await;
    assert_eq!(page.status(), StatusCode::OK);
    let html = body_text(page).await;
    assert!(html.contains("Pet 全部事件類型"));
    assert!(html.contains("data-acl-page=\"analytics-pet\""));
    assert!(!html.contains("data-acl-page=\"analytics-chat\""));
    // Independent user addition can grant a capability absent from the viewer role.
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) SELECT id,'analytics-accounts','allow',now() FROM uac_user WHERE user_acnt='reader'").execute(&pool).await.unwrap();
    let delegated = send(app.clone(), "/accounts/new", None, Some(&reader)).await;
    assert_eq!(delegated.status(), StatusCode::OK);
    assert!(!body_text(delegated).await.contains("action=\"/accounts\""));
    // A forged direct request must still fail even when the UI hides Create.
    use sha2::{Digest, Sha256};
    let raw = reader.strip_prefix("analytics_session=").unwrap();
    let csrf: String = Sha256::digest(format!("create-account:{raw}").as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    // A page allow does not imply its Create action.
    let denied=send(app.clone(),"/accounts",Some(&format!("username=pageonly&password=pageonly-password&email=pageonly%40example.test&pet=on&csrf={csrf}")),Some(&reader)).await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM uac_user WHERE user_acnt='pageonly'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) SELECT id,'analytics-accounts-create','allow',now() FROM uac_user WHERE user_acnt='reader'").execute(&pool).await.unwrap();
    let created=send(app.clone(),"/accounts",Some(&format!("username=delegated&password=delegated-password&email=delegated%40example.test&pet=on&csrf={csrf}")),Some(&reader)).await;
    assert_eq!(created.status(), StatusCode::SEE_OTHER);
    sqlx::query("UPDATE uac_user_acl SET acl_filter='deny' WHERE acl_id='analytics-accounts' AND user_id=(SELECT id FROM uac_user WHERE user_acnt='reader')").execute(&pool).await.unwrap();
    assert_eq!(
        send(app.clone(), "/accounts/new", None, Some(&reader))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(send(app.clone(),"/accounts",Some(&format!("username=denied&password=denied-password&email=denied%40example.test&pet=on&csrf={csrf}")),Some(&reader)).await.status(),StatusCode::FORBIDDEN);
    // Page permissions are role->ACL, independent from source grants.
    sqlx::query("DELETE FROM uac_role_acl WHERE role_id='analytics-viewer' AND acl_id='analytics-dashboard'").execute(&pool).await.unwrap();
    assert_eq!(
        send(app.clone(), "/dashboard", None, Some(&reader))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            app.clone(),
            "/api/v1/events/desktop_pet/summary",
            None,
            Some(&reader)
        )
        .await
        .status(),
        StatusCode::OK
    );
    sqlx::query("INSERT INTO uac_role_acl(role_id,acl_id,created_at) VALUES('analytics-viewer','analytics-dashboard',now())").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) SELECT id,'analytics-dashboard','deny',now() FROM uac_user WHERE user_acnt='reader'").execute(&pool).await.unwrap();
    assert_eq!(
        send(app.clone(), "/dashboard", None, Some(&reader))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE uac_user SET user_status='disabled' WHERE user_acnt='reader'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        send(
            app.clone(),
            "/api/v1/events/desktop_pet/summary",
            None,
            Some(&reader)
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let rejected:i64=sqlx::query_scalar("SELECT count(*) FROM analytics_account_audit WHERE action='create_viewer_rejected' AND result='409'").fetch_one(&pool).await.unwrap();
    assert_eq!(rejected, 2);
    // A per-user subtraction also applies to an administrator's default source grant.
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES($1,'analytics-chat','deny',now())").bind(&admin).execute(&pool).await.unwrap();
    assert_eq!(
        send(
            app.clone(),
            "/api/v1/events/web_chat/summary",
            None,
            Some(&cookie)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM analytics_account_audit WHERE actor_id=$1 AND action='create_viewer'",
    )
    .bind(admin)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit, 1);
}
