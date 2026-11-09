use crate::IngestState;
use crate::session_cookie::raw_cookie;
use analytics_application::login_attempt as attempts;
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
fn cookie(raw: &str, seconds: i64) -> String {
    format!("analytics_session={raw}; HttpOnly; SameSite=Strict; Path=/; Max-Age={seconds}")
}

pub(crate) fn renewed(response: impl IntoResponse, raw: &str, seconds: i64) -> Response {
    let mut response = response.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie(raw, seconds).parse().expect("validated hex token"),
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

fn login_feedback(status: StatusCode, name: &str, wait: &str) -> Response {
    let message = if wait == "0" {
        "帳號或密碼錯誤".to_owned()
    } else {
        format!("請等待 {wait} 秒後重試")
    };
    let mut res = (
        status,
        Html(crate::dashboard_ui::render_login_result(name, &message)),
    )
        .into_response();
    if wait != "0" {
        if let Ok(value) = wait.parse() {
            res.headers_mut().insert(header::RETRY_AFTER, value);
        }
    }
    res
}

async fn login(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Form(form): Form<Login>,
) -> Result<Response, StatusCode> {
    if headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) != Some(ORIGIN) {
        return Err(StatusCode::FORBIDDEN);
    }

    let username = form.username.trim().to_owned();
    if !analytics_domain::login_input::valid(&form.username, "admin") {
        return Ok(login_feedback(StatusCode::UNAUTHORIZED, "", "0"));
    }

    let mut attempt =
        analytics_persistence::login_attempt::LoginAttempt::begin(&state.pool, &username)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let Some(mut attempt) = attempt.take() else {
        return Ok(login_feedback(StatusCode::UNAUTHORIZED, &username, "0"));
    };
    let now = attempt
        .now()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let wait = attempts::pending(&mut attempt, now)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if wait != "0" {
        attempt
            .commit()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        return Ok(login_feedback(
            StatusCode::TOO_MANY_REQUESTS,
            &username,
            &wait,
        ));
    }

    let encoded = attempt.password_hash.clone();
    let fingerprint = digest(&encoded);
    let restricted = attempt.initial;
    let shape = analytics_domain::login_input::valid(&form.username, &form.password);
    let verified = if shape {
        tokio::task::spawn_blocking(move || {
            let parsed = PasswordHash::new(&encoded).map_err(|_| ())?;
            Argon2::default()
                .verify_password(form.password.as_bytes(), &parsed)
                .map_err(|_| ())
        })
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .is_ok()
    } else {
        false
    };
    if !verified {
        let now = attempt
            .now()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let wait = attempts::reject(&mut attempt, now)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        attempt
            .commit()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        return Ok(login_feedback(StatusCode::UNAUTHORIZED, &username, &wait));
    }
    attempts::accepted(&mut attempt)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let raw = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let seconds = attempt
        .session(&digest(&raw), &fingerprint)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    Ok(renewed(
        Redirect::to(if restricted {
            "/change-password"
        } else {
            "/dashboard"
        }),
        &raw,
        seconds,
    ))
}

// Recheck account status, mandatory-change state and password fingerprint on each protected request.
pub(crate) async fn session(
    state: &IngestState,
    headers: &HeaderMap,
) -> Result<(bool, String, i64), StatusCode> {
    let raw = raw_cookie(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let found = analytics_persistence::login_session::lookup(&state.pool, &digest(&raw))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let (restricted, must_change, current, fingerprint, id) =
        found.ok_or(StatusCode::UNAUTHORIZED)?;
    if digest(&current) != fingerprint {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let seconds =
        analytics_persistence::login_session::refresh(&state.pool, &digest(&raw), &id, &current)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
            .ok_or(StatusCode::UNAUTHORIZED)?;
    Ok((restricted || must_change, raw, seconds))
}

/// A live person Session and source grant are required on every business query.
pub(crate) async fn require_source_session(
    state: &IngestState,
    headers: &HeaderMap,
    source: &str,
) -> Result<(String, i64), StatusCode> {
    let (restricted, raw, seconds) = session(state, headers).await?;
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
    Ok((raw, seconds))
}

async fn change_page(
    State(state): State<IngestState>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let (restricted, raw, seconds) = session(&state, &headers).await?;
    let csrf = digest(&format!("change-password:{raw}"));
    let can_update = crate::page_acl::allowed(
        &state,
        &raw,
        "/change-password",
        crate::page_acl::Crud::Update,
    )
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let pages = crate::page_acl::pages(&state, &raw).await?;
    let html = crate::dashboard_ui::render_change_password_authorized(
        restricted,
        csrf,
        digest(&format!("logout:{raw}")),
        pages,
        can_update,
    );
    Ok(renewed(Html(html), &raw, seconds))
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
    let (_, raw, _seconds) = session(&state, &headers).await?;
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
    analytics_persistence::login_session::revoke_after_password_change(&mut tx, &id)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    analytics_persistence::account_audit::record_own_password_change(&mut tx, &id)
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
    let (restricted, raw, seconds) = match session(&state, &headers).await {
        Ok(v) => v,
        Err(StatusCode::UNAUTHORIZED) => return Redirect::to("/login").into_response(),
        Err(e) => return e.into_response(),
    };
    if restricted {
        return renewed(Redirect::to("/change-password"), &raw, seconds);
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

    let Some(source_label) = crate::dashboard_source::source_label(source) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    if let Err(status) = require_source_session(&state, &headers, source).await {
        return (status, "您沒有此來源的檢視權限。").into_response();
    }

    let kind = query.event_type.as_deref().unwrap_or("");
    if !kind.is_empty() && (!crate::valid_event_type(kind) || kind.contains('\0')) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let unavailable = || {
        let retry = format!(
            "/dashboard?source={source}&event_type={}",
            crate::dashboard_url::encode_query_component(kind)
        );
        renewed(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Html(crate::dashboard_ui::render_dashboard_unavailable(retry)),
            ),
            &raw,
            seconds,
        )
    };

    let rows = if kind.is_empty() {
        summary_for_source(&state.pool, source).await
    } else {
        summary_for_source_and_type(&state.pool, source, kind).await
    };
    let rows = match rows {
        Ok(v) => v,
        Err(_) => return unavailable(),
    };
    let Some(total) = crate::summary_total(&rows) else {
        return unavailable();
    };

    // 導覽連結依同一 Session 的即時來源權限決定，不能依畫面隱藏替代後端 ACL。
    let allow_chat = match require_source_session(&state, &headers, "web_chat").await {
        Ok(_) => true,
        Err(StatusCode::FORBIDDEN) => false,
        Err(status) => return status.into_response(),
    };
    let allow_pet = match require_source_session(&state, &headers, "desktop_pet").await {
        Ok(_) => true,
        Err(StatusCode::FORBIDDEN) => false,
        Err(status) => return status.into_response(),
    };
    let pages = match crate::page_acl::pages(&state, &raw).await {
        Ok(pages) => pages,
        Err(status) => return status.into_response(),
    };
    let can_export = match crate::page_acl::allowed(
        &state,
        &raw,
        "/api/v1/events/{source}/export.csv",
        crate::page_acl::Crud::Read,
    )
    .await
    {
        Ok(allowed) => allowed,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let visible_sources: Vec<String> = [("web_chat", allow_chat), ("desktop_pet", allow_pet)]
        .into_iter()
        .filter(|(_, allowed)| *allowed)
        .map(|(code, _)| code.to_owned())
        .collect();
    let visible_events: Vec<String> = rows.iter().map(|row| row.event_type.clone()).collect();
    let dictionary = match analytics_persistence::dashboard_dictionary(
        &state.pool,
        &visible_sources,
        &visible_events,
    )
    .await
    {
        Ok(dictionary) => dictionary,
        Err(_) => return unavailable(),
    };
    let html = crate::dashboard_ui::render_dashboard_dictionary(
        source_label,
        total,
        source,
        kind,
        &rows,
        &digest(&format!("logout:{raw}")),
        allow_chat,
        allow_pet,
        pages,
        can_export,
        dictionary,
    );
    renewed(Html(html), &raw, seconds)
}

async fn summary(
    State(state): State<IngestState>,
    Path(source): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let (raw, seconds) = require_source_session(&state, &headers, &source).await?;
    if !matches!(source.as_str(), "web_chat" | "desktop_pet") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let rows = summary_for_source(&state.pool, &source)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(renewed(Json(rows), &raw, seconds))
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

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionSettingsForm {
    idle_seconds: i64,
    csrf: String,
}

async fn session_settings_page(
    State(state): State<IngestState>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let (restricted, raw, seconds) = session(&state, &headers).await?;
    if restricted {
        return Err(StatusCode::FORBIDDEN);
    }
    let configured = analytics_persistence::session_settings::read(&state.pool)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let can_update = crate::page_acl::allowed(
        &state,
        &raw,
        "/settings/session",
        crate::page_acl::Crud::Update,
    )
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let pages = crate::page_acl::pages(&state, &raw).await?;
    let html = crate::dashboard_ui::render_session_settings(
        configured,
        digest(&format!("session-settings:{raw}")),
        digest(&format!("logout:{raw}")),
        pages,
        can_update,
    );
    Ok(renewed(Html(html), &raw, seconds))
}

async fn update_session_settings(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Form(form): Form<SessionSettingsForm>,
) -> Result<Response, StatusCode> {
    if headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some(ORIGIN) {
        return Err(StatusCode::FORBIDDEN);
    }
    let (restricted, raw, _) = session(&state, &headers).await?;
    if restricted || form.csrf != digest(&format!("session-settings:{raw}")) {
        return Err(StatusCode::FORBIDDEN);
    }
    if !analytics_domain::session_policy::valid_idle_seconds(form.idle_seconds) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    // 寫設定前在同一交易鎖定 Session 與帳號，並重新核對權限。
    let row: Option<(String, String, String, bool, bool)> = sqlx::query_as(
        "SELECT u.id,u.user_pd,s.credential_fingerprint,s.restricted,u.is_init_pd
         FROM uac_user u JOIN uac_login_session s ON s.user_id=u.id
         WHERE s.token_hash=$1
           AND s.expires_at>floor(extract(epoch from now()))::bigint
           AND u.user_status='active'
         FOR UPDATE OF u,s",
    )
    .bind(digest(&raw))
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let (actor, encoded, fingerprint, restricted, initial) = row.ok_or(StatusCode::UNAUTHORIZED)?;
    if restricted || initial {
        return Err(StatusCode::FORBIDDEN);
    }
    if digest(&encoded) != fingerprint {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let grant: bool = sqlx::query_scalar(crate::page_acl::PERMISSION_SQL)
        .bind(digest(&raw))
        .bind("/settings/session")
        .bind("update")
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !grant {
        return Err(StatusCode::FORBIDDEN);
    }
    analytics_persistence::session_settings::update(&mut tx, form.idle_seconds)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    sqlx::query(
        "INSERT INTO analytics_account_audit(id,actor_id,target_id,action,result)
         VALUES($1,$2,$2,'set_session_idle','updated')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(actor)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    // 讀回同一 policy 並再次刷新，成功回應 Cookie 與資料庫 TTL 一致。
    let (_, raw, seconds) = session(&state, &headers).await?;
    Ok(renewed(Redirect::to("/settings/session"), &raw, seconds))
}

pub fn build(state: IngestState) -> Router {
    Router::new()
    .route(
            "/assets/source-report.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("css/source_report.css"),
                )
            }),
        )
        .route("/navigation", get(crate::page_acl::navigation))
        .route(
            "/settings/session",
            get(session_settings_page).post(update_session_settings),
        )
        .route("/accounts/new", get(crate::accounts::page))
        .route("/accounts", post(crate::accounts::create))
        .route("/accounts/{id}/disable", post(crate::accounts::disable))
        .route(
            "/accounts/{id}/sources",
            post(crate::accounts::update_sources),
        )
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
