#![cfg(feature = "postgres-integration")]
mod support;
use analytics_server::{IngestState, IngestTokens, bootstrap, password_login};
use axum::http::StatusCode;
use std::sync::Arc;
use support::http::{body_text, form_csrf, response_cookie, send};

#[sqlx::test(migrations = "../migrations")]
async fn source_edits_take_effect_for_existing_session_and_respect_update_deny(pool: sqlx::PgPool) {
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
    let token = form_csrf(&page, "/accounts");
    assert_eq!(send(app.clone(),"/accounts",Some(&format!("username=reader47&password=reader-password&email=reader47%40example.test&chat=on&csrf={token}")),Some(&admin_cookie)).await.status(),StatusCode::SEE_OTHER);
    let id: String = sqlx::query_scalar("SELECT id FROM uac_user WHERE user_acnt='reader47'")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE uac_user SET is_init_pd=FALSE WHERE id=$1")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    let login = send(
        app.clone(),
        "/login",
        Some("username=reader47&password=reader-password"),
        None,
    )
    .await;
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
    let reader_cookie = response_cookie(&login);
    assert_eq!(
        send(
            app.clone(),
            "/api/v1/events/web_chat/summary",
            None,
            Some(&reader_cookie)
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            app.clone(),
            "/api/v1/events/desktop_pet/summary",
            None,
            Some(&reader_cookie)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let action = format!("/accounts/{id}/sources");
    let page = body_text(send(app.clone(), "/accounts/new", None, Some(&admin_cookie)).await).await;
    let token = form_csrf(&page, &action);
    assert_eq!(
        send(
            app.clone(),
            &action,
            Some("csrf=wrong&chat=deny&pet=allow"),
            Some(&admin_cookie)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            app.clone(),
            &action,
            Some(&format!("csrf={token}&chat=invalid&pet=allow")),
            Some(&admin_cookie)
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        send(
            app.clone(),
            &action,
            Some(&format!("csrf={token}&chat=deny&pet=allow")),
            Some(&admin_cookie)
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        send(
            app.clone(),
            "/api/v1/events/web_chat/summary",
            None,
            Some(&reader_cookie)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            app.clone(),
            "/api/v1/events/desktop_pet/summary",
            None,
            Some(&reader_cookie)
        )
        .await
        .status(),
        StatusCode::OK
    );
    let filters:Vec<(String,String)>=sqlx::query_as("SELECT acl_id,acl_filter FROM uac_user_acl WHERE user_id=$1 AND acl_id IN('analytics-chat','analytics-pet') ORDER BY acl_id").bind(&id).fetch_all(&pool).await.unwrap();
    assert_eq!(
        filters,
        vec![
            ("analytics-chat".into(), "deny".into()),
            ("analytics-pet".into(), "allow".into())
        ]
    );
    assert_eq!(
        send(
            app.clone(),
            &action,
            Some(&format!("csrf={token}&chat=inherit&pet=inherit")),
            Some(&admin_cookie)
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM uac_user_acl WHERE user_id=$1 AND acl_id IN('analytics-chat','analytics-pet')").bind(&id).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        send(
            app.clone(),
            "/api/v1/events/desktop_pet/summary",
            None,
            Some(&reader_cookie)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    sqlx::raw_sql("CREATE FUNCTION fail_source_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='update_viewer_sources' THEN RAISE EXCEPTION 'fixture audit failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_source_audit BEFORE INSERT ON analytics_account_audit FOR EACH ROW EXECUTE FUNCTION fail_source_audit();").execute(&pool).await.unwrap();
    assert_eq!(
        send(
            app.clone(),
            &action,
            Some(&format!("csrf={token}&chat=allow&pet=allow")),
            Some(&admin_cookie)
        )
        .await
        .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM uac_user_acl WHERE user_id=$1 AND acl_id IN('analytics-chat','analytics-pet')").bind(&id).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0);
    sqlx::raw_sql("DROP TRIGGER fail_source_audit ON analytics_account_audit; DROP FUNCTION fail_source_audit();").execute(&pool).await.unwrap();
    // 僅有action不會取得page；個人page與action加權不受角色名稱限制。
    sqlx::query("INSERT INTO uac_user(id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at) SELECT 'target47','target47',user_pd,'target47@example.test','目標','active',FALSE,now(),now() FROM uac_user WHERE id=$1").bind(&id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO uac_user_role(user_id,role_id,created_at) VALUES('target47','analytics-viewer',now())").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES($1,'analytics-accounts-update','allow',now())").bind(&id).execute(&pool).await.unwrap();
    use sha2::{Digest, Sha256};
    let reader_raw = reader_cookie.split_once('=').unwrap().1;
    let reader_token = Sha256::digest(format!("update-sources:{reader_raw}:target47"))
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(
        send(
            app.clone(),
            "/accounts/target47/sources",
            Some(&format!("csrf={reader_token}&chat=allow&pet=deny")),
            Some(&reader_cookie)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES($1,'analytics-accounts','allow',now())").bind(&id).execute(&pool).await.unwrap();
    let reader_page =
        body_text(send(app.clone(), "/accounts/new", None, Some(&reader_cookie)).await).await;
    let reader_token = form_csrf(&reader_page, "/accounts/target47/sources");
    assert_eq!(
        send(
            app.clone(),
            "/accounts/target47/sources",
            Some(&format!("csrf={reader_token}&chat=allow&pet=deny")),
            Some(&reader_cookie)
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES($1,'analytics-accounts-update','deny',now())").bind(&admin).execute(&pool).await.unwrap();
    let denied_page =
        body_text(send(app.clone(), "/accounts/new", None, Some(&admin_cookie)).await).await;
    assert!(!denied_page.contains(&format!("action=\"{action}\"")));
    assert_eq!(
        send(
            app.clone(),
            &action,
            Some(&format!("csrf={token}&chat=allow&pet=allow")),
            Some(&admin_cookie)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let audits:i64=sqlx::query_scalar("SELECT count(*) FROM analytics_account_audit WHERE target_id=$1 AND action='update_viewer_sources'").bind(&id).fetch_one(&pool).await.unwrap();
    assert_eq!(audits, 2);
    sqlx::query("DELETE FROM uac_user_acl WHERE user_id=$1 AND acl_id='analytics-accounts-update'")
        .bind(&admin)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO uac_user_role(user_id,role_id,created_at) VALUES($1,'analytics-admin',now())",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        send(
            app.clone(),
            &action,
            Some(&format!("csrf={token}&chat=allow&pet=allow")),
            Some(&admin_cookie)
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let page = body_text(send(app.clone(), "/accounts/new", None, Some(&admin_cookie)).await).await;
    assert!(!page.contains(&format!("action=\"{action}\"")));
    let raw = admin_cookie.split_once('=').unwrap().1;
    let token = Sha256::digest(format!("disable-account:{raw}:{id}"))
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(
        send(
            app.clone(),
            &format!("/accounts/{id}/disable"),
            Some(&format!("csrf={token}")),
            Some(&admin_cookie)
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let status: String = sqlx::query_scalar("SELECT user_status FROM uac_user WHERE id=$1")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "active");
}
