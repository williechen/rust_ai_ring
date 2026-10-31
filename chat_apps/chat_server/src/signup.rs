use argon2::{Argon2, PasswordHasher};
use axum::response::Response;
use axum::{
    Form, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::get,
};
use sqlx::SqlitePool;

#[derive(serde::Deserialize)]
struct Signup {
    username: String,
    email: String,
    display_name: String,
    password: String,
}

pub fn routes(pool: SqlitePool) -> Router {
    Router::new()
        .route("/signup", get(page).post(signup))
        .route("/signup/complete", get(complete))
        .layer(DefaultBodyLimit::max(4096))
        .with_state(pool)
}

async fn complete() -> impl IntoResponse {
    (
        [("cache-control", "no-store")],
        Html(
            r#"<!doctype html>
            <html lang="zh-Hant">
                <head>
                    <meta charset="utf-8">
                    <title>註冊完成</title>
                </head>
                <body>
                    <h1>註冊完成</h1>
                    <p>目前保持登出，請回首頁重新選擇聊天室，再登入。</p>
                    <a href="/">回首頁</a>
                </body>
            </html>"#,
        ),
    )
}

async fn page() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
        <html lang="zh-Hant">
            <head>
                <meta charset="utf-8">
                <title>會員註冊</title>
            </head>
            <body>
                <h1>會員註冊</h1>
                <form method="post" action="/signup">
                    <div style="margin-bottom: 1em;">
                        <label>帳號</label>
                        <input name="username" required minlength="3" maxlength="32" autocomplete="username">
                    </div>
                    <div style="margin-bottom: 1em;">
                        <label>Email</label>
                        <input name="email" type="email" required maxlength="254" autocomplete="email">
                    </div>
                    <div style="margin-bottom: 1em;"    >
                        <label>顯示名稱</label>
                        <input name="display_name" required maxlength="32">
                    </div>
                    <div>
                        <label>密碼</label>
                        <input name="password" type="password" required minlength="8" maxlength="128" autocomplete="new-password">
                    </div>
                    <button type="submit">建立會員</button>
                </form>
                <a href="/signin">回登入頁</a>
            </body>
        </html>"#,
    )
}

async fn signup(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
    Form(form): Form<Signup>,
) -> Result<Response, StatusCode> {
    // 檢查請求來源，僅允許來自 http://127.0.0.1:3000 的請求
    if headers.get("origin").and_then(|v| v.to_str().ok()) != Some("http://127.0.0.1:3000") {
        return Err(StatusCode::FORBIDDEN);
    }

    let username = form.username.trim().to_owned();
    let email = form.email.trim().to_ascii_lowercase();
    let display = form.display_name.trim().to_owned();
    // 保留新版畫面的 8-byte 密碼政策；所有限制都必須在伺服器重驗。
    // Email 僅做基本格式檢查，驗證信仍屬後續功能。
    let email_parts: Vec<_> = email.split('@').collect();
    if !(3..=32).contains(&username.len())
        || !username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        || !(1..=32).contains(&display.chars().count())
        || !email.is_ascii()
        || email.len() > 254
        || email_parts.len() != 2
        || email_parts[0].is_empty()
        || !email_parts[1].contains('.')
        || email_parts[1].starts_with('.')
        || email_parts[1].ends_with('.')
        || email
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        || !(8..=128).contains(&form.password.len())
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let password_hash = tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(form.password.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(|_| ())
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let member_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO mbr_user (id
            ,user_acnt
            ,user_pd
            ,user_email
            ,user_name
            ,user_status
            ,is_init_pd
            ,created_at
            ,updated_at
        ) VALUES (?1,?2,?3,?4,?5,'active',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)"#,
    )
    .bind(&member_id)
    .bind(username)
    .bind(password_hash)
    .bind(email)
    .bind(display)
    .execute(&mut *tx)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db) if db.is_unique_violation() => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    })?;

    sqlx::query("INSERT INTO mbr_user_role(user_id,role_id,created_at) VALUES(?1,'member-default',CURRENT_TIMESTAMP)")
        .bind(&member_id).execute(&mut *tx).await.map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
    // 成功註冊不沿用目前瀏覽器的帳號或選房；資料與撤銷同交易提交。
    if let Some(token) = headers
        .get("cookie")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| {
            h.split(';')
                .filter_map(|v| v.trim().split_once('='))
                .find(|(k, _)| *k == "chat_session")
                .map(|(_, v)| v)
        })
        .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        use sha2::{Digest, Sha256};
        let hashed: String = Sha256::digest(token.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        sqlx::query("DELETE FROM mbr_session WHERE token_hash=?1")
            .bind(hashed)
            .execute(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        [
            (
                "set-cookie",
                "chat_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
            ),
            ("cache-control", "no-store"),
        ],
        Redirect::to("/signup/complete"),
    )
        .into_response())
}

#[cfg(test)]
mod day22_tests {
    use super::*;
    use argon2::{PasswordHash, PasswordVerifier};
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn day22_signup_persists_and_rejects_duplicate() {
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
            "../../migrations/20261019221857_member_identity.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let app = routes(pool.clone());
        let body = "username=learner&email=learner%40example.test&display_name=Learner&password=lesson-only-password";
        for (input, expected) in [
            (body.to_owned(), StatusCode::SEE_OTHER),
            (body.replace("learner", "LEARNER"), StatusCode::CONFLICT),
            (
                body.replace("username=learner", "username=x"),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::post("/signup")
                        .header("origin", "http://127.0.0.1:3000")
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from(input))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            if expected == StatusCode::SEE_OTHER {
                assert_eq!(response.headers()["location"], "/signup/complete");
            }
        }
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM mbr_user")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
        let (hash,): (String,) = sqlx::query_as("SELECT user_pd FROM mbr_user")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(hash.starts_with("$argon2id$"));
        Argon2::default()
            .verify_password(b"lesson-only-password", &PasswordHash::new(&hash).unwrap())
            .unwrap();
        let response = app
            .oneshot(
                Request::post("/signup")
                    .header("origin", "http://evil.invalid")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

#[cfg(test)]
mod day23_correction_tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn day23_invalid_signup_is_rejected_before_db_write() {
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
            "../../migrations/20261019221857_member_identity.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let app = routes(pool.clone());
        let good =
            "username=learner&email=learner%40example.test&display_name=Learner&password=12345678";
        // DB 沒有既存帳號，422 不能靠重複 Email 的 409 掩蓋。
        for bad in [
            good.replace("username=learner", "username=x"),
            good.replace("username=learner", "username=bad-name"),
            good.replace("learner%40example.test", "not-an-email"),
            good.replace("display_name=Learner", "display_name=%20%20"),
            good.replace("password=12345678", "password=1234567"),
            good.replace("username=learner", &format!("username={}", "x".repeat(33))),
            good.replace(
                "display_name=Learner",
                &format!("display_name={}", "x".repeat(33)),
            ),
            good.replace(
                "password=12345678",
                &format!("password={}", "x".repeat(129)),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::post("/signup")
                        .header("origin", "http://127.0.0.1:3000")
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from(bad))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM mbr_user")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        // 下界與新版表單一致；保持 0.6 的自動隨機鹽 API。
        let response = app
            .oneshot(
                Request::post("/signup")
                    .header("origin", "http://127.0.0.1:3000")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(good))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
    }
}
