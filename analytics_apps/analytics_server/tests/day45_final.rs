#![cfg(feature = "postgres-integration")]
mod support;
use analytics_server::{IngestState, IngestTokens, bootstrap, password_login};
use axum::http::{StatusCode, header};
use std::sync::Arc;
use support::http::{body_text, response_cookie, send};

async fn fixture(pool: &sqlx::PgPool) -> (axum::Router, String, String) {
    let id = bootstrap::initialize(pool, "final-password".into())
        .await
        .unwrap();
    sqlx::query("INSERT INTO uac_user_role(user_id,role_id,created_at) VALUES($1,'analytics-admin',now()) ON CONFLICT DO NOTHING").bind(&id).execute(pool).await.unwrap();
    let app = password_login::build(IngestState {
        pool: pool.clone(),
        tokens: Arc::new(IngestTokens {
            chat: "chat-fixture".into(),
            pet: "pet-fixture".into(),
        }),
    });
    let response = send(
        app.clone(),
        "/login",
        Some("username=admin&password=final-password"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], "/change-password");
    (app, response_cookie(&response), id)
}

#[sqlx::test(migrations = "../migrations")]
async fn page_drives_category_and_actions_do_not_grant_pages(pool: sqlx::PgPool) {
    let (app, cookie, id) = fixture(&pool).await;
    sqlx::query("DELETE FROM uac_user_role WHERE user_id=$1")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES($1,'analytics-password','allow',now())").bind(&id).execute(&pool).await.unwrap();
    let response = send(app.clone(), "/change-password", None, Some(&cookie)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_text(response).await;
    assert!(
        html.contains("data-acl-category=\"analytics-members\""),
        "page-only must display category"
    );
    assert!(html.contains("data-acl-page=\"analytics-password\""));
    assert!(!html.contains("data-acl-action=\"analytics-password-update\""));
    assert!(
        !html.contains("action=\"/change-password\""),
        "no update => no form"
    );
    assert!(!html.contains("data-acl-category=\"analytics-info\""));
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES($1,'analytics-password-update','allow',now())").bind(&id).execute(&pool).await.unwrap();
    let html = body_text(send(app.clone(), "/change-password", None, Some(&cookie)).await).await;
    assert!(html.contains("data-acl-action=\"analytics-password-update\""));
    assert!(html.contains("action=\"/change-password\""));
    sqlx::query("UPDATE uac_user_acl SET acl_filter='deny' WHERE user_id=$1 AND acl_id='analytics-password'").bind(&id).execute(&pool).await.unwrap();
    assert_eq!(
        send(app.clone(), "/change-password", None, Some(&cookie))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let nav = body_text(send(app.clone(), "/navigation", None, Some(&cookie)).await).await;
    assert!(!nav.contains("data-acl-page=\"analytics-password\""));
    assert!(!nav.contains("data-acl-category=\"analytics-members\""));
    assert!(!nav.contains("data-acl-action=\"analytics-password-update\""));
    assert_eq!(
        send(app, "/dashboard", None, Some(&cookie)).await.status(),
        StatusCode::SEE_OTHER,
        "mandatory change cannot be bypassed by missing grants"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn deny_beats_roles_and_removing_override_restores_default(pool: sqlx::PgPool) {
    let (app, cookie, id) = fixture(&pool).await;
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES($1,'analytics-password-update','deny',now())").bind(&id).execute(&pool).await.unwrap();
    let before: (String, bool) =
        sqlx::query_as("SELECT user_pd,is_init_pd FROM uac_user WHERE id=$1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let raw = cookie.strip_prefix("analytics_session=").unwrap();
    use sha2::{Digest, Sha256};
    let csrf: String = Sha256::digest(format!("change-password:{raw}").as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let form = format!(
        "current_password=final-password&new_password=changed-password&confirm_password=changed-password&email=admin%40example.test&csrf={csrf}"
    );
    assert_eq!(
        send(app.clone(), "/change-password", Some(&form), Some(&cookie))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let after: (String, bool) =
        sqlx::query_as("SELECT user_pd,is_init_pd FROM uac_user WHERE id=$1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    sqlx::query("DELETE FROM uac_user_acl WHERE user_id=$1 AND acl_id='analytics-password-update'")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        send(app.clone(), "/change-password", Some(&form), Some(&cookie))
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let login = send(
        app.clone(),
        "/login",
        Some("username=admin&password=changed-password"),
        None,
    )
    .await;
    assert_eq!(login.headers()[header::LOCATION], "/dashboard");
    assert_eq!(
        send(app, "/change-password", None, Some(&cookie))
            .await
            .status(),
        StatusCode::UNAUTHORIZED,
        "all old sessions revoked"
    );
}

#[sqlx::test(migrations = false)]
async fn upgrade_from_exact_f6_migrations_preserves_checksums_and_deny(pool: sqlx::PgPool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../migrations");
    let dir = std::env::temp_dir().join(format!("day45-upgrade-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    for entry in std::fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap();
        if path.extension().is_some_and(|x| x == "sql") && name < "20261031120000" {
            let target = dir.join(path.file_name().unwrap());
            if name == "20261031074500_viewer_access.sql" {
                std::fs::write(
                    target,
                    exact_historical_fixture(
                        include_bytes!("fixtures/f6_viewer_access.sql"),
                        "fe81c4fd7b32a96e6b91f7a2532abf406ec88ff3a893350d57b0a2ee31ce5995",
                    ),
                )
                .unwrap();
            } else {
                std::fs::copy(&path, target).unwrap();
            }
        }
    }
    let old = sqlx::migrate::Migrator::new(dir.as_path()).await.unwrap();
    old.run(&pool).await.unwrap();
    let checksums: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(checksums.len(), 7);
    for id in ["initial-allow", "initial-deny"] {
        sqlx::query("INSERT INTO uac_user(id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at) VALUES($1,$1,'fixture-hash',$1||'@example.test',$1,'active',TRUE,now(),now())").bind(id).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES('initial-deny','analytics-password-update','deny',now())").execute(&pool).await.unwrap();
    let new = sqlx::migrate::Migrator::new(root.as_path()).await.unwrap();
    new.run(&pool).await.unwrap();
    new.run(&pool).await.unwrap();
    for (version, checksum) in checksums {
        let actual: Vec<u8> =
            sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version=$1")
                .bind(version)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(actual, checksum);
    }
    let grants:Vec<(String,String)>=sqlx::query_as("SELECT user_id,acl_filter FROM uac_user_acl WHERE acl_id='analytics-password-update' ORDER BY user_id").fetch_all(&pool).await.unwrap();
    assert_eq!(
        grants,
        vec![
            ("initial-allow".into(), "allow".into()),
            ("initial-deny".into(), "deny".into())
        ]
    );
    let viewer_update:i64=sqlx::query_scalar("SELECT count(*) FROM uac_role_acl WHERE role_id='analytics-viewer' AND acl_id='analytics-password-update'").fetch_one(&pool).await.unwrap();
    assert_eq!(viewer_update, 0);
    let definitions:(i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE acl_category='page'),count(*) FILTER(WHERE acl_category='action') FROM uac_acl").fetch_one(&pool).await.unwrap();
    assert_eq!(definitions, (11, 44));
    let grants:i64=sqlx::query_scalar("SELECT count(*) FROM uac_role_acl WHERE role_id='analytics-admin' AND (acl_id IN('analytics-roles','analytics-acls') OR acl_id IN(SELECT id FROM uac_acl WHERE acl_parent_id IN('analytics-roles','analytics-acls')))").fetch_one(&pool).await.unwrap();
    assert_eq!(grants, 10);
    std::fs::remove_dir_all(dir).unwrap();
}

#[sqlx::test(migrations = "../migrations")]
async fn active_role_union_personal_override_and_definition_state(pool: sqlx::PgPool) {
    let (app, cookie, id) = fixture(&pool).await;
    sqlx::query("DELETE FROM uac_user_role WHERE user_id=$1")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    for role in ["page-role", "action-role"] {
        sqlx::query("INSERT INTO uac_role(id,role_name,role_status,created_at,updated_at) VALUES($1,$1,'active',now(),now())").bind(role).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO uac_user_role(user_id,role_id,created_at) VALUES($1,$2,now())")
            .bind(&id)
            .bind(role)
            .execute(&pool)
            .await
            .unwrap();
    }
    for (role, acl) in [
        ("page-role", "analytics-password"),
        ("action-role", "analytics-password-update"),
    ] {
        sqlx::query("INSERT INTO uac_role_acl(role_id,acl_id,created_at) VALUES($1,$2,now())")
            .bind(role)
            .bind(acl)
            .execute(&pool)
            .await
            .unwrap();
    }
    let html = body_text(send(app.clone(), "/change-password", None, Some(&cookie)).await).await;
    assert!(html.contains("action=\"/change-password\""));
    sqlx::query("UPDATE uac_role SET role_status='inactive' WHERE id='action-role'")
        .execute(&pool)
        .await
        .unwrap();
    let html = body_text(send(app.clone(), "/change-password", None, Some(&cookie)).await).await;
    assert!(!html.contains("action=\"/change-password\""));
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES($1,'analytics-password-update','allow',now())").bind(&id).execute(&pool).await.unwrap();
    let html = body_text(send(app.clone(), "/change-password", None, Some(&cookie)).await).await;
    assert!(html.contains("action=\"/change-password\""));
    sqlx::query("UPDATE uac_acl SET acl_status='inactive' WHERE id='analytics-password-update'")
        .execute(&pool)
        .await
        .unwrap();
    let html = body_text(send(app.clone(), "/change-password", None, Some(&cookie)).await).await;
    assert!(!html.contains("action=\"/change-password\""));
}

#[sqlx::test(migrations = "../migrations")]
async fn restart_does_not_restore_revoked_role_and_tree_escapes_labels(pool: sqlx::PgPool) {
    let (app, cookie, id) = fixture(&pool).await;
    sqlx::query("DELETE FROM uac_user_role WHERE user_id=$1")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    bootstrap::ensure_default(&pool).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM uac_user_role WHERE user_id=$1")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES($1,'analytics-password','allow',now())").bind(&id).execute(&pool).await.unwrap();
    sqlx::query(
        "UPDATE uac_acl SET acl_name='<script>alert(1)</script>' WHERE id='analytics-password'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let html = body_text(send(app, "/navigation", None, Some(&cookie)).await).await;
    assert!(!html.contains("<script>"));
    assert!(html.contains("&lt;script&gt;"));
}

#[sqlx::test(migrations = false)]
async fn database_with_edited_28_checksum_is_not_silently_rewritten(pool: sqlx::PgPool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../migrations");
    let dir = std::env::temp_dir().join(format!("day45-edited-ledger-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    for entry in std::fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap();
        if path.extension().is_some_and(|e| e == "sql") && name < "20261031180000" {
            let target = dir.join(path.file_name().unwrap());
            if name == "20261031074500_viewer_access.sql" {
                std::fs::write(
                    target,
                    exact_historical_fixture(
                        include_bytes!("fixtures/28b71b6_viewer_access.sql"),
                        "6a97c57e0123fd9aa90c45a92ab43906958e5cf18bcd4dcae1fe064c19469857",
                    ),
                )
                .unwrap();
            } else {
                std::fs::copy(&path, target).unwrap();
            }
        }
    }
    sqlx::migrate::Migrator::new(dir.as_path())
        .await
        .unwrap()
        .run(&pool)
        .await
        .unwrap();
    let before: Vec<u8> =
        sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version=20261031074500")
            .fetch_one(&pool)
            .await
            .unwrap();
    let error = sqlx::migrate::Migrator::new(root.as_path())
        .await
        .unwrap()
        .run(&pool)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        sqlx::migrate::MigrateError::VersionMismatch(20261031074500)
    ));
    let after: Vec<u8> =
        sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version=20261031074500")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    std::fs::remove_dir_all(dir).unwrap();
}

// The two historical SQL files ended without LF. Editors may append one to the fixture.
// Accept only that known transport change, then pin the exact historical bytes by SHA256.
fn exact_historical_fixture(input: &[u8], expected: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let bytes = input.strip_suffix(b"\n").unwrap_or(input);
    let hash: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(hash, expected, "historical migration fixture bytes changed");
    bytes.to_vec()
}
