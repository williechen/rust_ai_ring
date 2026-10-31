//! Member self-service: actions only; identity and scope always come from the member Session.
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::{
    Form, Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use shared::auth::{cookie_token, token_digest};
use sqlx::SqlitePool;
pub const ROOM: &str = "member-room-create";
pub const PROFILE: &str = "member-self-profile-update";
pub const PASSWORD: &str = "member-self-password-update";

const PERMISSION: &str = r#"SELECT EXISTS(
    SELECT 1 
    FROM mbr_perm p 
    JOIN mbr_user u 
        ON u.id=?1
    WHERE p.id=?2 
        AND p.perm_status='active' 
        AND u.user_status='active'
        AND NOT EXISTS(
            SELECT 1 
            FROM mbr_user_perm x 
            WHERE x.user_id=u.id 
                AND x.perm_id=p.id 
                AND x.perm_filter='deny'
        )
        AND (
            EXISTS(
                SELECT 1 
                FROM mbr_user_perm x 
                WHERE x.user_id=u.id 
                    AND x.perm_id=p.id 
                    AND x.perm_filter='allow'
            )
            OR EXISTS(
                SELECT 1 
                FROM mbr_user_role ur 
                JOIN mbr_role r 
                    ON r.id=ur.role_id
                JOIN mbr_role_perm rp 
                    ON rp.role_id=r.id 
                    AND rp.perm_id=p.id
                WHERE ur.user_id=u.id 
                    AND r.role_status='active'
            )
        )
    )"#;

fn db_error(e: sqlx::Error) -> StatusCode {
    match e {
        sqlx::Error::Database(d) if d.is_unique_violation() => StatusCode::CONFLICT,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn raw(headers: &HeaderMap) -> Result<String, StatusCode> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| cookie_token(v, "chat_session"))
        .ok_or(StatusCode::UNAUTHORIZED)
}

fn csrf(headers: &HeaderMap, operation: &str) -> Result<String, StatusCode> {
    Ok(token_digest(&format!("{operation}:{}", raw(headers)?)))
}

fn verify_csrf(headers: &HeaderMap, operation: &str, value: &str) -> Result<(), StatusCode> {
    if headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) != Some("http://127.0.0.1:3000")
        || csrf(headers, operation)? != value
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

pub async fn allowed(pool: &SqlitePool, user: &str, action: &str) -> Result<bool, StatusCode> {
    sqlx::query_scalar(PERMISSION)
        .bind(user)
        .bind(action)
        .fetch_one(pool)
        .await
        .map_err(db_error)
}

pub async fn require_action(
    pool: &SqlitePool,
    headers: &HeaderMap,
    action: &str,
) -> Result<String, StatusCode> {
    let user = crate::auth_session::active_member(pool, headers).await?;
    if !allowed(pool, &user, action).await? {
        return Err(StatusCode::FORBIDDEN);
    }
    let initial: bool = sqlx::query_scalar("SELECT is_init_pd FROM mbr_user WHERE id=?1")
        .bind(&user)
        .fetch_one(pool)
        .await
        .map_err(db_error)?;
    if initial && action != PASSWORD {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(user)
}

// Revalidate identity and action in the same SQLite transaction as the mutation.
async fn transaction_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    headers: &HeaderMap,
    action: &str,
) -> Result<(String, String), StatusCode> {
    let row:Option<(String,String,bool)>=sqlx::query_as("SELECT u.id,u.user_pd,u.is_init_pd FROM mbr_user u JOIN mbr_session s ON s.mbr_user_id=u.id WHERE s.token_hash=?1 AND s.expires_at>unixepoch() AND u.user_status='active'").bind(token_digest(&raw(headers)?)).fetch_optional(&mut **tx).await.map_err(db_error)?;
    let (id, hash, initial) = row.ok_or(StatusCode::UNAUTHORIZED)?;
    let permit: bool = sqlx::query_scalar(PERMISSION)
        .bind(&id)
        .bind(action)
        .fetch_one(&mut **tx)
        .await
        .map_err(db_error)?;
    if !permit || (initial && action != PASSWORD) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok((id, hash))
}
pub fn routes(pool: SqlitePool) -> Router {
    Router::new()
        .route("/api/member-capabilities", get(capabilities))
        .route("/account/profile", get(profile_page).post(profile))
        .route("/account/password", get(password_page).post(password))
        .route_layer(axum::middleware::from_fn(reject_target_query))
        .layer(DefaultBodyLimit::max(4096))
        .with_state(pool)
}

fn page(title: &str, form: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><html lang=\"zh-Hant\"><head><meta charset=\"utf-8\"><title>{title}</title></head><body><main><h1>{title}</h1>{form}<a href=\"/account\">個人管理</a></main></body></html>"
    ))
}

async fn profile_page(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    require_action(&pool, &headers, PROFILE).await?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        page(
            "修改本人資料",
            &format!(
                "<form method=\"post\" action=\"/account/profile\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><label>顯示名稱<input name=\"display_name\" required maxlength=\"32\"></label><label>Email<input type=\"email\" name=\"email\" required maxlength=\"254\"></label><button>儲存本人資料</button></form>",
                csrf(&headers, "member-profile")?
            ),
        ),
    ))
}

async fn password_page(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    let user = require_action(&pool, &headers, PASSWORD).await?;
    let initial: bool = sqlx::query_scalar("SELECT is_init_pd FROM mbr_user WHERE id=?1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .map_err(db_error)?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        page(
            if initial {
                "首次登入須修改本人密碼"
            } else {
                "修改本人密碼"
            },
            &format!(
                "<form method=\"post\" action=\"/account/password\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><label>目前密碼<input type=\"password\" name=\"current_password\" autocomplete=\"current-password\" required maxlength=\"128\"></label><label>新密碼<input type=\"password\" name=\"new_password\" autocomplete=\"new-password\" required minlength=\"8\" maxlength=\"128\"></label><label>確認新密碼<input type=\"password\" name=\"confirm_password\" autocomplete=\"new-password\" required minlength=\"8\" maxlength=\"128\"></label><button>修改本人密碼</button></form>",
                csrf(&headers, "member-password")?
            ),
        ),
    ))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    display_name: String,
    email: String,
    csrf: String,
}

async fn profile(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
    Form(form): Form<Profile>,
) -> Result<Response, StatusCode> {
    require_action(&pool, &headers, PROFILE).await?;
    verify_csrf(&headers, "member-profile", &form.csrf)?;
    let name = form.display_name.trim();
    let email = form.email.trim().to_ascii_lowercase();
    let parts: Vec<_> = email.split('@').collect();
    if !(1..=32).contains(&name.chars().count())
        || name.chars().any(char::is_control)
        || !email.is_ascii()
        || email.len() > 254
        || email
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        || parts.len() != 2
        || parts[0].is_empty()
        || !parts[1].contains('.')
        || parts[1].starts_with('.')
        || parts[1].ends_with('.')
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let mut tx = pool.begin().await.map_err(db_error)?;
    let (id, _) = transaction_user(&mut tx, &headers, PROFILE).await?;
    sqlx::query(
        "UPDATE mbr_user SET user_name=?1,user_email=?2,updated_at=CURRENT_TIMESTAMP WHERE id=?3",
    )
    .bind(name)
    .bind(email)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Redirect::to("/account"),
    )
        .into_response())
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Password {
    current_password: String,
    new_password: String,
    confirm_password: String,
    csrf: String,
}

async fn password(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
    Form(form): Form<Password>,
) -> Result<Response, StatusCode> {
    let user = require_action(&pool, &headers, PASSWORD).await?;
    verify_csrf(&headers, "member-password", &form.csrf)?;
    if form.current_password.is_empty()
        || form.current_password.len() > 128
        || !(8..=128).contains(&form.new_password.len())
        || form.new_password != form.confirm_password
        || form.new_password == form.current_password
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let current: String = sqlx::query_scalar("SELECT user_pd FROM mbr_user WHERE id=?1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .map_err(db_error)?;
    let checked = current.clone();
    let encoded = tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&checked).map_err(|_| StatusCode::UNAUTHORIZED)?;
        Argon2::default()
            .verify_password(form.current_password.as_bytes(), &parsed)
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
        Argon2::default()
            .hash_password(form.new_password.as_bytes())
            .map(|h| h.to_string())
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
    })
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)??;
    let mut tx = pool.begin().await.map_err(db_error)?;
    let (id, hash) = transaction_user(&mut tx, &headers, PASSWORD).await?;
    if hash != current {
        return Err(StatusCode::UNAUTHORIZED);
    }
    sqlx::query(
        "UPDATE mbr_user SET user_pd=?1,is_init_pd=FALSE,updated_at=CURRENT_TIMESTAMP WHERE id=?2",
    )
    .bind(encoded)
    .bind(&id)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    sqlx::query("DELETE FROM mbr_session WHERE mbr_user_id=?1")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok((
        [
            (
                header::SET_COOKIE,
                "chat_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Redirect::to("/signin"),
    )
        .into_response())
}

pub async fn create_room(
    pool: &SqlitePool,
    headers: &HeaderMap,
    name: String,
) -> Result<String, StatusCode> {
    require_action(pool, headers, ROOM).await?;
    if chat_application::validate_room_name(&name).is_err() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let mut tx = pool.begin().await.map_err(db_error)?;
    transaction_user(&mut tx, headers, ROOM).await?;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO rooms(id,category_room_id,name,desc,created_at,updated_at) VALUES(?1,'default-category',?2,'',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").bind(&id).bind(name).execute(&mut *tx).await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(id)
}

pub async fn create_room_api(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
    Json(input): Json<shared::CreateRoomRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    require_action(&pool, &headers, ROOM).await?;
    let token = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::FORBIDDEN)?;
    verify_csrf(&headers, "create-room", token)?;
    let id = create_room(&pool, &headers, input.name).await?;
    Ok((StatusCode::CREATED, Json(shared::CreateRoomResponse { id })))
}

pub async fn account_markup(
    pool: &SqlitePool,
    headers: &HeaderMap,
    mut html: String,
) -> Result<String, StatusCode> {
    let user = crate::auth_session::active_member(pool, headers).await?;
    let initial: bool = sqlx::query_scalar("SELECT is_init_pd FROM mbr_user WHERE id=?1")
        .bind(&user)
        .fetch_one(pool)
        .await
        .map_err(db_error)?;
    if !allowed(pool, &user, ROOM).await? || initial {
        if let Some(start) = html.find("<form method=\"post\" action=\"/account/rooms\"") {
            if let Some(end) = html[start..].find("</form>") {
                html.replace_range(start..start + end + 7, "");
            }
        }
    }
    let mut links = String::new();
    for (action, path, label) in [
        (PROFILE, "/account/profile", "修改本人資料"),
        (PASSWORD, "/account/password", "修改本人密碼"),
    ] {
        if allowed(pool, &user, action).await? && (!initial || action == PASSWORD) {
            links.push_str(&format!("<a href=\"{path}\">{label}</a>"));
        }
    }
    Ok(html.replacen("</main>", &format!("{links}</main>"), 1))
}

pub async fn initial(pool: &SqlitePool, user: &str) -> Result<bool, StatusCode> {
    sqlx::query_scalar("SELECT is_init_pd FROM mbr_user WHERE id=?1")
        .bind(user)
        .fetch_one(pool)
        .await
        .map_err(db_error)
}

#[derive(serde::Serialize)]
struct Capabilities {
    create_room: bool,
    update_profile: bool,
    update_password: bool,
    create_csrf: String,
}

async fn capabilities(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    let user = crate::auth_session::active_member(&pool, &headers).await?;
    let restricted = initial(&pool, &user).await?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(Capabilities {
            create_room: !restricted && allowed(&pool, &user, ROOM).await?,
            update_profile: !restricted && allowed(&pool, &user, PROFILE).await?,
            update_password: allowed(&pool, &user, PASSWORD).await?,
            create_csrf: csrf(&headers, "create-room")?,
        }),
    ))
}

async fn reject_target_query(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<Response, StatusCode> {
    if request.uri().query().is_some_and(|q| !q.is_empty()) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    Ok(next.run(request).await)
}
