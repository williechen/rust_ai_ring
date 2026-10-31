use crate::IngestState;
use crate::session_cookie::raw_cookie;
use analytics_persistence::summary_for_source;
use analytics_persistence::summary_for_source_and_type;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::{
    Form, Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const ORIGIN: &str = "http://127.0.0.1:3100";

pub(crate) fn digest(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn cookie(raw: &str) -> String {
    format!("analytics_session={raw}; HttpOnly; SameSite=Strict; Path=/; Max-Age=1800")
}

pub(crate) fn renewed(response: impl IntoResponse, raw: &str) -> Response {
    let mut response = response.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie(raw).parse().expect("validated hex token"),
    );
    response
}

#[derive(Deserialize)]
struct Login {
    username: String,
    password: String,
}

async fn login_page() -> Html<String> {
    Html(crate::dashboard_ui::render_login())
}

async fn login(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Form(form): Form<Login>,
) -> Result<Response, StatusCode> {
    if headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) != Some(ORIGIN) {
        return Err(StatusCode::FORBIDDEN);
    }

    if form.username.len() > 64
        || form.username.trim().is_empty()
        || !form
            .username
            .trim()
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        || !(form.password == "admin" || (8..=128).contains(&form.password.len()))
    {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Reject duplicate case-folded identities rather than guessing one.
    let found: Vec<(String, String, bool, String)> = sqlx::query_as(
        r#"SELECT id
            ,user_pd
            ,is_init_pd
            ,user_status
        FROM uac_user
        WHERE lower(user_acnt)=lower($1)
        LIMIT 2"#,
    )
    .bind(form.username.trim())
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if found.len() != 1 {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let (id, encoded, restricted, status) = found.into_iter().next().unwrap();
    if status != "active" {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let fingerprint = digest(&encoded);
    let verified = tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&encoded).map_err(|_| ())?;
        Argon2::default()
            .verify_password(form.password.as_bytes(), &parsed)
            .map_err(|_| ())
    })
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    verified.map_err(|_| StatusCode::UNAUTHORIZED)?;

    let raw = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    sqlx::query(
        r#"DELETE FROM uac_login_session
        WHERE expires_at <= floor(extract(epoch from now()))::bigint"#,
    )
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    sqlx::query(
        r#"INSERT INTO uac_login_session(token_hash
            ,user_id
            ,credential_fingerprint
            ,restricted
            ,expires_at
        ) VALUES ($1,$2,$3,$4,floor(extract(epoch from now()))::bigint+1800)"#,
    )
    .bind(digest(&raw))
    .bind(id)
    .bind(fingerprint)
    .bind(restricted)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    Ok(renewed(
        Redirect::to(if restricted {
            "/change-password"
        } else {
            "/dashboard"
        }),
        &raw,
    ))
}

// Recheck account status, mandatory-change state and password fingerprint on each protected request.
pub(crate) async fn session(
    state: &IngestState,
    headers: &HeaderMap,
) -> Result<(bool, String), StatusCode> {
    let raw = raw_cookie(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let found = analytics_persistence::login_session::lookup(&state.pool, &digest(&raw))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let (restricted, must_change, current, fingerprint, id) =
        found.ok_or(StatusCode::UNAUTHORIZED)?;
    if digest(&current) != fingerprint {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let refreshed = sqlx::query(
        r#"UPDATE uac_login_session
            SET expires_at=floor(extract(epoch from now()))::bigint+1800
        WHERE token_hash=$1
            AND expires_at>floor(extract(epoch from now()))::bigint
            AND EXISTS (
                SELECT 1
                FROM uac_user
                WHERE id=$2
                    AND user_pd=$3 
                    AND user_status='active')"#,
    )
    .bind(digest(&raw))
    .bind(id)
    .bind(current)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if refreshed.rows_affected() != 1 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok((restricted || must_change, raw))
}

/// A live person Session and source grant are required on every business query.
pub(crate) async fn require_source_session(
    state: &IngestState,
    headers: &HeaderMap,
    source: &str,
) -> Result<String, StatusCode> {
    let (restricted, raw) = session(state, headers).await?;
    if restricted {
        return Err(StatusCode::FORBIDDEN);
    }

    if !matches!(source, "web_chat" | "desktop_pet") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let path = match source {
        "web_chat" => "/api/v1/events/web_chat",
        "desktop_pet" => "/api/v1/events/desktop_pet",
        _ => unreachable!(),
    };

    let allowed = crate::page_acl::allowed(state, &raw, path, crate::page_acl::Crud::Read)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    if !allowed {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(raw)
}

async fn change_page(
    State(state): State<IngestState>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let (restricted, raw) = session(&state, &headers).await?;
    let csrf = digest(&format!("change-password:{raw}"));
    let can_update = crate::page_acl::allowed(
        &state,
        &raw,
        "/change-password",
        crate::page_acl::Crud::Update,
    )
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut html = crate::dashboard_ui::render_change_password_with_logout(
        restricted,
        csrf,
        digest(&format!("logout:{raw}")),
    );
    if !can_update {
        if let Some(start) = html.find("<form method=\"post\" action=\"/change-password\"") {
            if let Some(end) = html[start..].find("</form>") {
                html.replace_range(
                    start..start + end + 7,
                    "<p>沒有修改密碼 action；請聯絡管理員處理授權。</p>",
                );
            }
        }
    }
    Ok(renewed(
        Html(crate::page_acl::decorate(&state, &raw, html).await?),
        &raw,
    ))
}

#[derive(Deserialize)]
struct PasswordChange {
    current_password: String,
    new_password: String,
    confirm_password: String,
    email: String,
    csrf: String,
}

async fn change_password(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Form(form): Form<PasswordChange>,
) -> Result<Response, StatusCode> {
    if headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) != Some(ORIGIN) {
        return Err(StatusCode::FORBIDDEN);
    }
    let (_, raw) = session(&state, &headers).await?;
    if form.csrf != digest(&format!("change-password:{raw}")) {
        return Err(StatusCode::FORBIDDEN);
    }
    let email = form.email.trim().to_ascii_lowercase();
    let pieces: Vec<_> = email.split('@').collect();
    if !(8..=128).contains(&form.new_password.len())
        || form.current_password.is_empty()
        || form.current_password.len() > 128
        || form.new_password != form.confirm_password
        || form.new_password == form.current_password
        || email.len() > 254
        || !email.is_ascii()
        || email
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        || pieces.len() != 2
        || pieces[0].is_empty()
        || !pieces[1].contains('.')
        || pieces[1].starts_with('.')
        || pieces[1].ends_with('.')
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    // 重驗本次有效 Session，鎖定會員與 Session，不能靠前端或先前 GET 的結果授權。
    let row: Option<(String, String, String)> = sqlx::query_as(
        r#"SELECT u.id
            ,u.user_pd
            ,s.credential_fingerprint
        FROM uac_user u
        JOIN uac_login_session s
            ON s.user_id=u.id
        WHERE s.token_hash=$1
            AND s.expires_at>floor(extract(epoch from now()))::bigint
            AND u.user_status='active' 
        FOR UPDATE OF u,s"#,
    )
    .bind(digest(&raw))
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let (id, current, fingerprint) = row.ok_or(StatusCode::UNAUTHORIZED)?;
    if digest(&current) != fingerprint {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let permit: bool = sqlx::query_scalar(crate::page_acl::PERMISSION_SQL)
        .bind(digest(&raw))
        .bind("/change-password")
        .bind("update")
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !permit {
        return Err(StatusCode::FORBIDDEN);
    }

    let encoded = tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&current).map_err(|_| StatusCode::UNAUTHORIZED)?;
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
    sqlx::query(
        r#"UPDATE uac_user
            SET user_pd=$1
                ,user_email=$2
                ,is_init_pd=FALSE
                ,updated_at=now()
        WHERE id=$3"#,
    )
    .bind(encoded)
    .bind(email)
    .bind(&id)
    .execute(&mut *tx)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref d) if d.is_unique_violation() => StatusCode::CONFLICT,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    })?;
    sqlx::query("DELETE FROM uac_login_session WHERE user_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok((
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, "/login"),
            (
                header::SET_COOKIE,
                "analytics_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
            ),
        ],
    )
        .into_response())
}

#[derive(Default, Deserialize)]
struct DashboardSourceQuery {
    source: Option<String>,
    event_type: Option<String>,
}

async fn dashboard(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Query(query): Query<DashboardSourceQuery>,
) -> Response {
    let (restricted, raw) = match session(&state, &headers).await {
        Ok(v) => v,
        Err(StatusCode::UNAUTHORIZED) => return Redirect::to("/login").into_response(),
        Err(e) => return e.into_response(),
    };
    if restricted {
        return renewed(Redirect::to("/change-password"), &raw);
    }

    let default_source = if query.source.is_none() {
        match require_source_session(&state, &headers, "web_chat").await {
            Ok(_) => "web_chat",
            Err(StatusCode::FORBIDDEN) => "desktop_pet",
            Err(status) => return status.into_response(),
        }
    } else {
        "web_chat"
    };
    let source = query.source.as_deref().unwrap_or(default_source);
    let source_label = match source {
        "web_chat" => "Chat",
        "desktop_pet" => "Pet",
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    if let Err(status) = require_source_session(&state, &headers, source).await {
        return (status, "您沒有此來源的檢視權限。").into_response();
    }
    let kind = query.event_type.as_deref().unwrap_or("");
    if !kind.is_empty() && (!crate::valid_event_type(kind) || kind.contains('\0')) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let rows = if kind.is_empty() {
        summary_for_source(&state.pool, source).await
    } else {
        summary_for_source_and_type(&state.pool, source, kind).await
    };
    let rows = match rows {
        Ok(v) => v,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let Some(total) = crate::summary_total(&rows) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    let html = crate::dashboard_ui::render_dashboard_with_rows_csrf(
        source_label,
        total,
        source,
        kind,
        &rows,
        &digest(&format!("logout:{raw}")),
    );
    match crate::page_acl::decorate(&state, &raw, html).await {
        Ok(html) => renewed(Html(html), &raw),
        Err(e) => e.into_response(),
    }
}

async fn summary(
    State(state): State<IngestState>,
    Path(source): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let raw = require_source_session(&state, &headers, &source).await?;
    if !matches!(source.as_str(), "web_chat" | "desktop_pet") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let rows = summary_for_source(&state.pool, &source)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(renewed(Json(rows), &raw))
}

#[derive(Deserialize)]
struct LogoutForm {
    csrf: String,
}

async fn logout(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Form(form): Form<LogoutForm>,
) -> Result<Response, StatusCode> {
    if headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) != Some(ORIGIN) {
        return Err(StatusCode::FORBIDDEN);
    }
    let raw = raw_cookie(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if form.csrf != digest(&format!("logout:{raw}")) {
        return Err(StatusCode::FORBIDDEN);
    }
    sqlx::query("DELETE FROM uac_login_session WHERE token_hash=$1")
        .bind(digest(&raw))
        .execute(&state.pool)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok((
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, "/login"),
            (
                header::SET_COOKIE,
                "analytics_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
            ),
        ],
    )
        .into_response())
}

pub fn build(state: IngestState) -> Router {
    Router::new()
        .route("/navigation", get(crate::page_acl::navigation))
        .route("/accounts/new", get(crate::accounts::page))
        .route("/accounts", post(crate::accounts::create))
        .route("/login", get(login_page).post(login))
        .route("/logout", post(logout))
        .route("/change-password", get(change_page).post(change_password))
        .route("/", get(dashboard))
        .route("/dashboard", get(dashboard))
        .route("/api/v1/events/{source}/summary", get(summary))
        .route(
            "/api/v1/events/{source}/export.csv",
            get(crate::export::export_summary),
        )
        .route(
            "/api/v1/events/{source}/summary/{event_type}",
            get(crate::source_kind_summary),
        )
        .route("/healthz", get(crate::health))
        .route("/api/v1/events", post(crate::ingest))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::page_acl::require,
        ))
        .layer(DefaultBodyLimit::max(4096))
        .layer(axum::middleware::map_response(
            |mut r: Response| async move {
                r.headers_mut()
                    .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
                r
            },
        ))
        .with_state(state)
}
