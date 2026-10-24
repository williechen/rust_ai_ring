use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    Form, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

fn token_digest(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
}

pub fn routes(pool: SqlitePool) -> Router {
    Router::new()
        .route("/login", get(login_page).post(login))
        .route("/account", get(account))
        .route("/account/rooms", axum::routing::post(account_create_room))
        .layer(DefaultBodyLimit::max(4096))
        .with_state(pool)
}

async fn login_page() -> Redirect {
    Redirect::to("/signin")
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
    if name.is_empty() || name.len() > 32 || input.password.is_empty() || input.password.len() > 128
    {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let found: Option<(String, String, bool)> = sqlx::query_as(
        r#"SELECT id
                ,user_pd
                ,(user_status='active') 
            FROM mbr_user 
            WHERE lower(user_acnt)=lower(?1)"#,
    )
    .bind(name)
    .fetch_optional(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (user_id, hash, active) = found.ok_or(StatusCode::UNAUTHORIZED)?;
    if !active {
        return Err(StatusCode::UNAUTHORIZED);
    }
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
        return Err(StatusCode::UNAUTHORIZED);
    }

    // 兩個獨立 UUIDv4 組成不可預測的 session bearer；不直接入庫。
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let hashed = token_digest(&token);
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO mbr_session (id,mbr_user_id,token_hash,expires_at) VALUES (?1,?2,?3,unixepoch()+86400)"
    ).bind(id).bind(user_id).bind(hashed).execute(&pool).await
        .map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    let cookie = format!("chat_session={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age=86400");
    Ok((
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, "/account"),
            (header::SET_COOKIE, cookie.as_str()),
        ],
    )
        .into_response())
}

fn account_html(
    headers: &HeaderMap,
    message: String,
    name: String,
    room_id: Option<String>,
) -> Result<Html<String>, StatusCode> {
    Ok(Html(chat_ui::account::render_account(
        crate::auth_session::csrf_for_logout(headers)?,
        crate::auth_session::csrf_for_create_room(headers)?,
        message,
        name,
        room_id,
    )))
}

async fn account(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    crate::auth_session::active_member(&pool, &headers).await?;
    account_html(&headers, String::new(), String::new(), None)
}

#[derive(Deserialize)]
struct AccountRoomForm {
    name: String,
    csrf: String,
}

async fn account_create_room(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
    Form(form): Form<AccountRoomForm>,
) -> Result<Response, StatusCode> {
    crate::auth_session::active_member(&pool, &headers).await?;
    if headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) != Some("http://127.0.0.1:3000")
        || form.csrf != crate::auth_session::csrf_for_create_room(&headers)?
    {
        return Err(StatusCode::FORBIDDEN);
    }

    if let Err(message) = chat_application::validate_room_name(&form.name) {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            account_html(&headers, message.into(), form.name, None)?,
        )
            .into_response());
    }
    let repo = chat_persistence::SqliteRoomRepository::new(pool);
    let created = chat_application::create_room(
        &repo,
        chat_application::CreateRoom {
            id: uuid::Uuid::new_v4().to_string(),
            category_room_id: "default-category".into(),
            name: form.name.clone(),
        },
    )
    .await;
    match created {
        Ok(room) => Ok((
            StatusCode::CREATED,
            account_html(
                &headers,
                "已建立；可進房或回公開清單".into(),
                String::new(),
                Some(room.id),
            )?,
        )
            .into_response()),
        Err(_) => Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            account_html(
                &headers,
                "未確認建立成功；請先查清單再決定是否重送".into(),
                form.name,
                None,
            )?,
        )
            .into_response()),
    }
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
        sqlx::raw_sql(include_str!(
            "../../migrations/20261018073931_create_auth_user.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::raw_sql(include_str!(
            "../../migrations/20261020152737_mbr_sessions.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
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
