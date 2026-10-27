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

fn digest(value: &str) -> String {
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

    if !form.username.trim().eq_ignore_ascii_case("admin")
        || !(form.password == "admin" || (8..=128).contains(&form.password.len()))
    {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Existing bootstrap only provisions admin. Reject duplicate case-folded identities rather than guessing one.
    let found: Vec<(String, String, bool, String)> = sqlx::query_as(
        r#"SELECT id
            ,user_pd
            ,is_init_pd
            ,user_status 
        FROM uac_user 
        WHERE lower(user_acnt)='admin' 
        LIMIT 2"#,
    )
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
async fn session(state: &IngestState, headers: &HeaderMap) -> Result<(bool, String), StatusCode> {
    let raw = raw_cookie(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let found: Option<(bool, bool, String, String, String)> = sqlx::query_as(
        r#"SELECT s.restricted
            ,u.is_init_pd
            ,u.user_pd
            ,s.credential_fingerprint
            ,u.id 
        FROM uac_login_session s 
        JOIN uac_user u 
            ON u.id=s.user_id 
        WHERE s.token_hash=$1 
            AND s.expires_at>floor(extract(epoch from now()))::bigint 
            AND u.user_status='active' 
            AND lower(u.user_acnt)='admin'"#,
    )
    .bind(digest(&raw))
    .fetch_optional(&state.pool)
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

/// 已完成首次改密碼的有效管理者，才可讀取業務統計。
/// 角色與來源 ACL 尚未建置，不得冒稱檢視者已獲授權。
pub(crate) async fn require_query_session(
    state: &IngestState,
    headers: &HeaderMap,
) -> Result<String, StatusCode> {
    let (restricted, raw) = session(state, headers).await?;
    if restricted {
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
    Ok(renewed(
        Html(crate::dashboard_ui::render_change_password(
            restricted, csrf,
        )),
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
            AND lower(u.user_acnt)='admin' 
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

    let source = query.source.as_deref().unwrap_or("web_chat");
    let source_label = match source {
        "web_chat" => "Chat",
        "desktop_pet" => "Pet",
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
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

    renewed(
        Html(crate::dashboard_ui::render_dashboard_with_rows(
            source_label,
            total,
            source,
            kind,
            &rows,
        )),
        &raw,
    )
}

async fn summary(
    State(state): State<IngestState>,
    Path(source): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let raw = require_query_session(&state, &headers).await?;
    if !matches!(source.as_str(), "web_chat" | "desktop_pet") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let rows = summary_for_source(&state.pool, &source)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(renewed(Json(rows), &raw))
}

async fn logout(
    State(state): State<IngestState>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    if headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) != Some(ORIGIN) {
        return Err(StatusCode::FORBIDDEN);
    }
    let raw = raw_cookie(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
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
