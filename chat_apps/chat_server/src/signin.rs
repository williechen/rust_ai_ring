use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    Form, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use serde::Deserialize;
use shared::auth::token_digest;
use sqlx::SqlitePool;

use crate::room_path::canonical_room_id;

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
    #[serde(default)]
    next: String,
}

pub fn routes(pool: SqlitePool) -> Router {
    Router::new()
        .route("/login", get(login_page).post(login))
        .route("/account", get(account))
        .route("/api/session", get(session_status))
        .route("/account/rooms", axum::routing::post(account_create_room))
        .layer(DefaultBodyLimit::max(4096))
        .with_state(pool.clone())
        .merge(crate::member_self::routes(pool))
}

/// 同步延長有效工作階段與瀏覽器 Cookie。
async fn session_status(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let cookie = crate::auth_session::renew_member_cookie(&pool, &headers).await?;
    Ok((
        StatusCode::NO_CONTENT,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::SET_COOKIE, cookie.as_str()),
        ],
    )
        .into_response())
}

/// Reject external URLs, fragments, queries and noncanonical room IDs.
fn safe_room_return(next: &str) -> Option<&str> {
    canonical_room_id(next).map(|_| next)
}

async fn login_page() -> Redirect {
    Redirect::to("/signin")
}

fn login_failure(username: &str, next: &str) -> Response {
    // 只保留可顯示帳號與站內房間；Leptos負責HTML escaping。
    let username = if username.len() <= 32 {
        username.to_owned()
    } else {
        String::new()
    };
    let next = safe_room_return(next).unwrap_or("").to_owned();
    (
        StatusCode::UNAUTHORIZED,
        [(header::CACHE_CONTROL, "no-store")],
        Html(chat_ui::signin_form::render_login_failure(username, next)),
    )
        .into_response()
}

fn login_wait(username: &str, next: &str, seconds: String, invalid: bool) -> Response {
    let target = safe_room_return(next).unwrap_or("").to_owned();
    let status = if invalid {
        StatusCode::UNAUTHORIZED
    } else {
        StatusCode::TOO_MANY_REQUESTS
    };
    (
        status,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::RETRY_AFTER, seconds.as_str()),
        ],
        Html(chat_ui::signin_form::render_login_wait(
            username.to_owned(),
            target,
            seconds.clone(),
            invalid,
        )),
    )
        .into_response()
}

async fn login(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
    Form(input): Form<Credentials>,
) -> Result<Response, StatusCode> {
    if headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some("http://127.0.0.1:3000") {
        return Err(StatusCode::FORBIDDEN);
    }

    let name = input.username.trim();
    let valid = chat_domain::login_input::valid_credentials(name, &input.password);
    if name.is_empty() || name.len() > 32 {
        return Ok(login_failure(name, &input.next));
    }

    let Some(mut attempt) = chat_persistence::login_attempt::LoginAttempt::begin(&pool, name)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    else {
        return Ok(login_failure(name, &input.next));
    };

    let now = attempt
        .now()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let wait = chat_application::login_attempt::pending(&mut attempt, now)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if wait != "0" {
        attempt
            .commit()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        return Ok(login_wait(name, &input.next, wait, false));
    }
    if !valid {
        let now = attempt
            .now()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let wait = chat_application::login_attempt::reject(&mut attempt, now)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        attempt
            .commit()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        return Ok(login_wait(name, &input.next, wait, true));
    }
    let hash = attempt.password_hash.clone();
    let initial = attempt.initial;
    let password = input.password;
    let verified = tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&hash).map_err(|_| ())?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .map(|_| true)
            .map_err(|_| ())
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if verified.is_err() {
        let now = attempt
            .now()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let wait = chat_application::login_attempt::reject(&mut attempt, now)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        attempt
            .commit()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        return Ok(login_wait(name, &input.next, wait, true));
    }
    chat_application::login_attempt::accepted(&mut attempt)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    // 兩個獨立 UUIDv4 組成不可預測的 session bearer；不直接入庫。
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let hashed = token_digest(&token);
    let id = uuid::Uuid::new_v4().to_string();
    attempt
        .session(&id, &hashed)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let cookie = crate::auth_session::member_cookie(&token);

    let destination = if initial {
        "/account/password"
    } else {
        safe_room_return(&input.next).unwrap_or("/account")
    };

    Ok((
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, destination),
            (header::SET_COOKIE, cookie.as_str()),
        ],
    )
        .into_response())
}

async fn account_html(
    pool: &SqlitePool,
    headers: &HeaderMap,
    message: String,
    name: String,
    room_id: Option<String>,
    selected_category: String,
) -> Result<Html<String>, StatusCode> {
    let categories: Vec<(String, String)> = sqlx::query_as(
        "SELECT code,value FROM codemap WHERE category='room_category' ORDER BY CASE WHEN code='default-category' THEN 0 ELSE 1 END,code"
    ).fetch_all(pool).await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Html(chat_ui::account::render_account_categories(
        crate::auth_session::csrf_for_logout(headers)?,
        crate::auth_session::csrf_for_create_room(headers)?,
        message,
        name,
        room_id,
        categories,
        selected_category,
    )))
}

async fn account(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    let user = crate::auth_session::active_member(&pool, &headers).await?;
    let initial: bool = sqlx::query_scalar("SELECT is_init_pd FROM mbr_user WHERE id=?1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if initial {
        return Err(StatusCode::FORBIDDEN);
    }
    let Html(html) = account_html(
        &pool,
        &headers,
        String::new(),
        String::new(),
        None,
        "default-category".into(),
    )
    .await?;
    Ok(Html(
        crate::member_self::account_markup(&pool, &headers, html).await?,
    ))
}

#[derive(Deserialize)]
struct AccountRoomForm {
    name: String,
    csrf: String,
    category: Option<String>,
}

async fn account_create_room(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
    Form(form): Form<AccountRoomForm>,
) -> Result<Response, StatusCode> {
    crate::member_self::require_action(&pool, &headers, crate::member_self::ROOM).await?;
    if headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) != Some("http://127.0.0.1:3000")
        || form.csrf != crate::auth_session::csrf_for_create_room(&headers)?
    {
        return Err(StatusCode::FORBIDDEN);
    }

    let category = form.category.as_deref().unwrap_or("default-category");
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM codemap WHERE category='room_category' AND code=?1)",
    )
    .bind(category)
    .fetch_one(&pool)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let validation = chat_application::validate_room_name(&form.name).and_then(|()| {
        if exists {
            Ok(())
        } else {
            Err("請選擇有效的聊天室分類")
        }
    });
    if let Err(message) = validation {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            account_html(
                &pool,
                &headers,
                message.into(),
                form.name,
                None,
                category.into(),
            )
            .await?,
        )
            .into_response());
    }
    let id =
        crate::member_self::create_room_in_category(&pool, &headers, form.name, category).await?;
    let Html(html) = account_html(
        &pool,
        &headers,
        "已建立；可進房或回公開清單".into(),
        String::new(),
        Some(id),
        category.into(),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Html(crate::member_self::account_markup(&pool, &headers, html).await?),
    )
        .into_response())
}

#[cfg(test)]
mod day23_tests {
    use super::*;
    use argon2::PasswordHasher;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn day23_password_login_creates_persisted_session_and_guards_account() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        chat_persistence::migrate(&pool).await.unwrap();

        let encoded = argon2::Argon2::default()
            .hash_password(b"lesson-only-password")
            .unwrap()
            .to_string();

        sqlx::query(
            "INSERT INTO mbr_user (id,user_acnt,user_pd,user_email,user_name,user_status,user_sexy,user_birthday,is_init_pd,created_at,updated_at) VALUES ('member-1','learner',?1,'learner@example.test','Learner','active',NULL,NULL,FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)"
        ).bind(encoded).execute(&pool).await.unwrap();

        let app = routes(pool.clone());

        let denied = app
            .clone()
            .oneshot(Request::get("/account").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

        let bad = app
            .clone()
            .oneshot(
                Request::post("/login")
                    .header("origin", "http://127.0.0.1:3000")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("username=learner&password=wrong-password"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);

        sqlx::query("UPDATE mbr_login_attempt SET blocked_until='0'")
            .execute(&pool)
            .await
            .unwrap();
        let ok = app
            .clone()
            .oneshot(
                Request::post("/login")
                    .header("origin", "http://127.0.0.1:3000")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("username=LEARNER&password=lesson-only-password"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::SEE_OTHER);
        assert_eq!(ok.headers()["location"], "/account");

        let cookie = ok.headers()[axum::http::header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();

        let allowed = app
            .clone()
            .oneshot(
                Request::get("/account")
                    .header(axum::http::header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);

        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM mbr_session")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
        // The no-next /account, wrong-password, Cookie and persistence assertions remain.
        let id = uuid::Uuid::new_v4();
        let next = format!("%2Froom%2F{id}");
        let return_login = app
            .clone()
            .oneshot(
                Request::post("/login")
                    .header("origin", "http://127.0.0.1:3000")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(format!(
                        "username=LEARNER&password=lesson-only-password&next={next}"
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(return_login.status(), StatusCode::SEE_OTHER);
        assert_eq!(return_login.headers()["location"], format!("/room/{id}"));
        let hostile = app.oneshot(Request::post("/login")
            .header("origin", "http://127.0.0.1:3000")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("username=LEARNER&password=lesson-only-password&next=https%3A%2F%2Fevil.invalid"))
            .unwrap()).await.unwrap();
        assert_eq!(hostile.headers()["location"], "/account");
    }
}

#[cfg(test)]
mod day34_login_redirect_tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn legacy_login_get_redirects_without_rendering_another_form() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        let response = super::routes(pool)
            .oneshot(Request::get("/login").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()["location"], "/signin");
        let body = to_bytes(response.into_body(), 16384).await.unwrap();
        assert!(!std::str::from_utf8(&body).unwrap().contains("<form"));
    }
}

#[cfg(test)]
mod day35_account_room_tests {
    use super::*;
    use axum::http::header;

    #[tokio::test]
    async fn logged_in_account_rejects_newline_without_room_write() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        chat_persistence::migrate(&pool).await.unwrap();
        sqlx::query("INSERT INTO mbr_user (id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at) VALUES ('day35','learner35','unused','day35@example.test','Learner','active',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO mbr_user_role(user_id,role_id,created_at) VALUES('day35','member-default',CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        let raw = "b".repeat(64);
        sqlx::query("INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at) VALUES ('s35','day35',?1,unixepoch()+60)")
            .bind(shared::auth::token_digest(&raw))
            .execute(&pool).await.unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("chat_session={raw}").parse().unwrap(),
        );
        headers.insert(header::ORIGIN, "http://127.0.0.1:3000".parse().unwrap());
        let csrf = crate::auth_session::csrf_for_create_room(&headers).unwrap();
        let response = account_create_room(
            State(pool.clone()),
            headers,
            Form(AccountRoomForm {
                name: "房\n間".into(),
                csrf,
                category: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM rooms")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        pool.close().await;
    }
}

#[cfg(test)]
mod day40_session_entry_tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn session_endpoint_checks_active_revoked_and_expired_sqlite_session() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        chat_persistence::migrate(&pool).await.unwrap();
        sqlx::query("INSERT INTO mbr_user(id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at)
            VALUES('day40-member','day40','hash','day40@example.invalid','Day40','active',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)")
            .execute(&pool).await.unwrap();
        let raw = "a".repeat(64);
        sqlx::query(
            "INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at)
            VALUES('day40-session','day40-member',?1,unixepoch()+60)",
        )
        .bind(shared::auth::token_digest(&raw))
        .execute(&pool)
        .await
        .unwrap();
        let app = routes(pool.clone());
        let anonymous = app
            .clone()
            .oneshot(Request::get("/api/session").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
        let live = app
            .clone()
            .oneshot(
                Request::get("/api/session")
                    .header("cookie", format!("chat_session={raw}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(live.status(), StatusCode::NO_CONTENT);
        assert_eq!(live.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(
            live.headers()
                .get(header::SET_COOKIE)
                .and_then(|value| value.to_str().ok()),
            Some(
                format!("chat_session={raw}; HttpOnly; SameSite=Lax; Path=/; Max-Age=86400")
                    .as_str()
            )
        );
        let remaining: i64 = sqlx::query_scalar(
            "SELECT expires_at-unixepoch() FROM mbr_session WHERE id='day40-session'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!((86_390..=86_400).contains(&remaining));
        sqlx::query("UPDATE mbr_user SET user_status='disabled' WHERE id='day40-member'")
            .execute(&pool)
            .await
            .unwrap();
        let disabled = app
            .clone()
            .oneshot(
                Request::get("/api/session")
                    .header("cookie", format!("chat_session={raw}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(disabled.status(), StatusCode::UNAUTHORIZED);
        assert!(!disabled.headers().contains_key(header::SET_COOKIE));
        sqlx::query("UPDATE mbr_user SET user_status='active' WHERE id='day40-member'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE mbr_session SET expires_at=unixepoch()-1 WHERE id='day40-session'")
            .execute(&pool)
            .await
            .unwrap();
        let expired = app
            .oneshot(
                Request::get("/api/session")
                    .header("cookie", format!("chat_session={raw}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(expired.status(), StatusCode::UNAUTHORIZED);
        assert!(!expired.headers().contains_key(header::SET_COOKIE));
        let still_expired: bool = sqlx::query_scalar(
            "SELECT expires_at<=unixepoch() FROM mbr_session WHERE id='day40-session'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(still_expired);
    }
}
