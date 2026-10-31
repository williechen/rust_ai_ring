use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use shared::auth::token_digest;
use tower::ServiceExt;

async fn send(
    app: Router,
    path: &str,
    body: Option<String>,
    cookie: &str,
) -> axum::response::Response {
    let mut req = Request::builder()
        .method(if body.is_some() { "POST" } else { "GET" })
        .uri(path)
        .header("cookie", cookie)
        .header("origin", "http://127.0.0.1:3000");
    if body.is_some() {
        req = req.header("content-type", "application/x-www-form-urlencoded");
    }
    app.oneshot(req.body(Body::from(body.unwrap_or_default())).unwrap())
        .await
        .unwrap()
}

async fn fixture() -> (sqlx::SqlitePool, Router, String) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    chat_persistence::migrate(&pool).await.unwrap();
    let hash = Argon2::default()
        .hash_password(b"member-password")
        .unwrap()
        .to_string();
    for id in ["self-member", "other-member"] {
        sqlx::query("INSERT INTO mbr_user(id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at) VALUES(?1,?1,?2,?1||'@example.test',?1,'active',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").bind(id).bind(&hash).execute(&pool).await.unwrap();
    }
    for (id, name) in [
        ("member-room-create", "建立聊天室"),
        ("member-profile-update", "修改本人資料"),
        ("member-password-update", "修改本人密碼"),
    ] {
        sqlx::query("INSERT INTO mbr_perm(id,perm_name,perm_status,created_at,updated_at) VALUES(?1,?2,'active',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP) ON CONFLICT DO NOTHING").bind(id).bind(name).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO mbr_user_perm(user_id,perm_id,perm_filter,created_at) VALUES('self-member',?1,'allow',CURRENT_TIMESTAMP)").bind(id).execute(&pool).await.unwrap();
    }
    let raw = "a".repeat(64);
    sqlx::query("INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at) VALUES('fixture-session','self-member',?1,unixepoch()+600)").bind(token_digest(&raw)).execute(&pool).await.unwrap();
    let app = chat_server::signin::routes(pool.clone());
    (pool, app, format!("chat_session={raw}"))
}

fn csrf(cookie: &str, operation: &str) -> String {
    token_digest(&format!(
        "{operation}:{}",
        cookie.strip_prefix("chat_session=").unwrap()
    ))
}

#[tokio::test]
async fn self_profile_is_separate_and_cannot_target_another_member() {
    let (pool, app, cookie) = fixture().await;
    assert_eq!(
        send(app.clone(), "/account/profile", None, &cookie)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            app.clone(),
            "/account/profile?user_id=other-member",
            None,
            &cookie
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let payload = format!(
        "display_name=Changed&email=changed%40example.test&csrf={}",
        csrf(&cookie, "member-profile")
    );
    assert_eq!(
        send(
            app.clone(),
            "/account/profile",
            Some(format!("{payload}&user_id=other-member")),
            &cookie
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let unchanged: String =
        sqlx::query_scalar("SELECT user_name FROM mbr_user WHERE id='other-member'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unchanged, "other-member");
    assert_eq!(
        send(
            app.clone(),
            "/account/profile",
            Some(payload.clone()),
            &cookie
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    let name: String = sqlx::query_scalar("SELECT user_name FROM mbr_user WHERE id='self-member'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "Changed");
    sqlx::query("UPDATE mbr_user_perm SET perm_filter='deny' WHERE user_id='self-member' AND perm_id='member-profile-update'").execute(&pool).await.unwrap();
    assert_eq!(
        send(app.clone(), "/account/profile", Some(payload), &cookie)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(app, "/account/password", None, &cookie).await.status(),
        StatusCode::OK,
        "profile deny must not remove password action"
    );
}

#[tokio::test]
async fn room_creation_requires_action_and_deny_leaves_no_room() {
    let (pool, app, cookie) = fixture().await;
    sqlx::query("UPDATE mbr_user_perm SET perm_filter='deny' WHERE user_id='self-member' AND perm_id='member-room-create'").execute(&pool).await.unwrap();
    let form = format!("name=Forbidden&csrf={}", csrf(&cookie, "create-room"));
    assert_eq!(
        send(app.clone(), "/account/rooms", Some(form), &cookie)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = send(app.clone(), "/api/member-capabilities", None, &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["create_room"], false);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM rooms")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let body = to_bytes(
        send(app, "/account", None, &cookie).await.into_body(),
        65536,
    )
    .await
    .unwrap();
    assert!(!String::from_utf8_lossy(&body).contains("action=\"/account/rooms\""));
}

#[tokio::test]
async fn password_change_is_self_only_csrf_checked_salted_and_revokes_sessions() {
    let (pool, app, cookie) = fixture().await;
    let form = format!(
        "current_password=member-password&new_password=changed-password&confirm_password=changed-password&csrf={}",
        csrf(&cookie, "member-password")
    );
    assert_eq!(
        send(
            app.clone(),
            "/account/password",
            Some(format!("{form}&user_id=other-member")),
            &cookie
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        send(
            app.clone(),
            "/account/password",
            Some(form.replace(&csrf(&cookie, "member-password"), "wrong")),
            &cookie
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(app.clone(), "/account/password", Some(form), &cookie)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let hash: String = sqlx::query_scalar("SELECT user_pd FROM mbr_user WHERE id='self-member'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(hash.starts_with("$argon2id$"));
    Argon2::default()
        .verify_password(b"changed-password", &PasswordHash::new(&hash).unwrap())
        .unwrap();
    let other: String = sqlx::query_scalar("SELECT user_pd FROM mbr_user WHERE id='other-member'")
        .fetch_one(&pool)
        .await
        .unwrap();
    Argon2::default()
        .verify_password(b"member-password", &PasswordHash::new(&other).unwrap())
        .unwrap();
    assert_eq!(
        send(app, "/account/password", None, &cookie).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn member_roles_union_overrides_and_disabled_definitions() {
    let (pool, _, cookie) = fixture().await;
    sqlx::query("DELETE FROM mbr_user_perm WHERE user_id='self-member'")
        .execute(&pool)
        .await
        .unwrap();
    for (role, perm) in [
        ("role-a", "member-room-create"),
        ("role-b", "member-profile-update"),
    ] {
        sqlx::query("INSERT INTO mbr_role(id,role_name,role_status,created_at,updated_at) VALUES(?1,?1,'active',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").bind(role).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO mbr_user_role(user_id,role_id,created_at) VALUES('self-member',?1,CURRENT_TIMESTAMP)").bind(role).execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO mbr_role_perm(role_id,perm_id,created_at) VALUES(?1,?2,CURRENT_TIMESTAMP)",
        )
        .bind(role)
        .bind(perm)
        .execute(&pool)
        .await
        .unwrap();
    }
    use chat_server::member_self::{PASSWORD, PROFILE, ROOM, allowed};
    assert!(allowed(&pool, "self-member", ROOM).await.unwrap());
    assert!(allowed(&pool, "self-member", PROFILE).await.unwrap());
    assert!(!allowed(&pool, "self-member", PASSWORD).await.unwrap());
    sqlx::query("INSERT INTO mbr_user_perm(user_id,perm_id,perm_filter,created_at) VALUES('self-member','member-room-create','deny',CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
    assert!(!allowed(&pool, "self-member", ROOM).await.unwrap());
    sqlx::query("DELETE FROM mbr_user_perm WHERE user_id='self-member'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(allowed(&pool, "self-member", ROOM).await.unwrap());
    sqlx::query("UPDATE mbr_role SET role_status='inactive' WHERE id='role-a'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!allowed(&pool, "self-member", ROOM).await.unwrap());
    sqlx::query("UPDATE mbr_perm SET perm_status='inactive' WHERE id='member-profile-update'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!allowed(&pool, "self-member", PROFILE).await.unwrap());
    assert!(cookie.starts_with("chat_session="));
}

#[tokio::test]
async fn mandatory_change_cannot_be_bypassed_and_next_login_opens_account() {
    let (pool, app, cookie) = fixture().await;
    sqlx::query("UPDATE mbr_user SET is_init_pd=TRUE WHERE id='self-member'")
        .execute(&pool)
        .await
        .unwrap();
    let response = send(
        app.clone(),
        "/login",
        Some("username=self-member&password=member-password".into()),
        &cookie,
    )
    .await;
    assert_eq!(response.headers()["location"], "/account/password");
    assert_eq!(
        send(app.clone(), "/account", None, &cookie).await.status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE mbr_user_perm SET perm_filter='deny' WHERE user_id='self-member' AND perm_id='member-password-update'").execute(&pool).await.unwrap();
    assert_eq!(
        send(app.clone(), "/account/password", None, &cookie)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE mbr_user_perm SET perm_filter='allow' WHERE user_id='self-member' AND perm_id='member-password-update'").execute(&pool).await.unwrap();
    let form = format!(
        "current_password=member-password&new_password=changed-password&confirm_password=changed-password&csrf={}",
        csrf(&cookie, "member-password")
    );
    assert_eq!(
        send(app.clone(), "/account/password", Some(form), &cookie)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let response = send(
        app,
        "/login",
        Some("username=self-member&password=changed-password".into()),
        &cookie,
    )
    .await;
    assert_eq!(response.headers()["location"], "/account");
}

#[tokio::test]
async fn json_room_route_uses_same_action_and_csrf() {
    let (pool, app, cookie) = fixture().await;
    let app = app.merge(
        Router::new()
            .route(
                "/api/rooms",
                axum::routing::post(chat_server::member_self::create_room_api),
            )
            .with_state(pool.clone()),
    );
    for (token, expected) in [
        ("wrong".into(), StatusCode::FORBIDDEN),
        (csrf(&cookie, "create-room"), StatusCode::CREATED),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/rooms")
                    .header("cookie", &cookie)
                    .header("origin", "http://127.0.0.1:3000")
                    .header("content-type", "application/json")
                    .header("x-csrf-token", token)
                    .body(Body::from(r#"{"name":"Allowed"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    sqlx::query("UPDATE mbr_user_perm SET perm_filter='deny' WHERE user_id='self-member' AND perm_id='member-room-create'").execute(&pool).await.unwrap();
    let response = app
        .oneshot(
            Request::post("/api/rooms")
                .header("cookie", &cookie)
                .header("origin", "http://127.0.0.1:3000")
                .header("content-type", "application/json")
                .header("x-csrf-token", csrf(&cookie, "create-room"))
                .body(Body::from(r#"{"name":"Denied"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM rooms")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn failed_session_revocation_rolls_password_back() {
    let (pool, app, cookie) = fixture().await;
    let before: String = sqlx::query_scalar("SELECT user_pd FROM mbr_user WHERE id='self-member'")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TRIGGER fail_session_delete BEFORE DELETE ON mbr_session BEGIN SELECT RAISE(ABORT,'fixture revoke failure'); END;").execute(&pool).await.unwrap();
    let form = format!(
        "current_password=member-password&new_password=changed-password&confirm_password=changed-password&csrf={}",
        csrf(&cookie, "member-password")
    );
    assert_eq!(
        send(app, "/account/password", Some(form), &cookie)
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let after: String = sqlx::query_scalar("SELECT user_pd FROM mbr_user WHERE id='self-member'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM mbr_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sessions, 1);
}
