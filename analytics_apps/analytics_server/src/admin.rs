use crate::IngestState;
use analytics_persistence::summary_for_source;
use analytics_persistence::{EventSummary, summary_for_source_and_type};
use axum::{
    Form, Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use leptos::prelude::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use uuid::Uuid;

const ORIGIN: &str = "http://127.0.0.1:3100";
const TTL: Duration = Duration::from_secs(1800);

// 僅對仍有效且已通過 cookie 查找的 Session 延長閒置期限。
fn touch_session(session: &mut Session, now: Instant) {
    session.expires = now + TTL;
}

fn hash(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn same(a: &str, b: &str) -> bool {
    let mut difference = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        difference |= usize::from(
            a.as_bytes().get(i).copied().unwrap_or(0) ^ b.as_bytes().get(i).copied().unwrap_or(0),
        );
    }
    difference == 0
}

#[derive(Clone)]
pub struct Config {
    key_hash: String,
}

impl Config {
    pub fn new(key_hash: String) -> Result<Self, &'static str> {
        if key_hash.len() != 64 || !key_hash.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("管理員登入碼雜湊設定不合法");
        }
        Ok(Self {
            key_hash: key_hash.to_lowercase(),
        })
    }
    pub fn from_env() -> Result<Self, &'static str> {
        Self::new(
            std::env::var("ANALYTICS_ADMIN_KEY_SHA256")
                .map_err(|_| "缺少 ANALYTICS_ADMIN_KEY_SHA256")?,
        )
    }
}

struct Session {
    expires: Instant,
    csrf: String,
}

#[derive(Clone)]
struct Admin {
    ingest: IngestState,
    config: Option<Config>,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    attempts: Arc<Mutex<(Instant, u8)>>,
}

fn origin_ok(headers: &HeaderMap) -> bool {
    headers.get(header::ORIGIN).and_then(|s| s.to_str().ok()) == Some(ORIGIN)
}

fn cookie_value(headers: &HeaderMap) -> Option<&str> {
    let value = headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|v| v.trim().split_once('='))
        .find(|(k, _)| *k == "analytics_session")?
        .1;
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(value)
}

fn cookie_key(headers: &HeaderMap) -> Option<String> {
    cookie_value(headers).map(hash)
}

fn csrf_for(state: &Admin, headers: &HeaderMap) -> Option<String> {
    let key = cookie_key(headers)?;
    let now = Instant::now();
    let mut sessions = state.sessions.lock().ok()?;
    sessions.retain(|_, session| session.expires > now);
    let session = sessions.get(&key)?;
    Some(session.csrf.clone())
}

// Refresh both expiry clocks only after a successful protected GET. Logout and
// login never pass through this layer. Recheck after the handler so a revoked or
// expired session cannot be resurrected by an in-flight response.
fn session_cookie(raw: &str) -> axum::http::HeaderValue {
    format!(
        "analytics_session={raw}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        TTL.as_secs()
    )
    .parse()
    .expect("validated hex session cookie")
}

async fn refresh_session(
    State(state): State<Admin>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let raw = if request.method() == axum::http::Method::GET {
        cookie_value(request.headers()).map(str::to_owned)
    } else {
        None
    };
    let mut response = next.run(request).await;
    if response.status().is_success()
        && let Some(raw) = raw
        && let Ok(mut sessions) = state.sessions.lock()
    {
        let now = Instant::now();
        sessions.retain(|_, session| session.expires > now);
        if let Some(session) = sessions.get_mut(&hash(&raw)) {
            touch_session(session, now);
            response
                .headers_mut()
                .insert(header::SET_COOKIE, session_cookie(&raw));
        }
    }
    response
}

fn document(body: String) -> Html<String> {
    Html(format!(
        r#"<!doctype html>
        <html lang="zh-Hant">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width,initial-scale=1">
                <title>Analytics 管理後台</title>
                <style>
                    body{{
                        font:16px system-ui;
                        max-width:1000px;
                        margin:32px auto;
                        padding:16px;
                        background:#f4f6fa;
                        color:#243247;
                    }}
                    input,button,select{{
                        font:inherit;
                        padding:10px;
                        margin:8px;
                    }}
                    table{{
                        background:white;
                        width:100%;
                        border-collapse:collapse;
                    }}
                    td,th{{
                        padding:12px;
                        border:1px solid #ddd;
                        text-align:left;
                        overflow-wrap:anywhere;
                    }}
                    form{{margin:18px 0}}
                    label{{display:inline-block}}
                </style>
            </head>
            <body>{body}</body>
        </html>"#
    ))
}

async fn login_page() -> Html<String> {
    document(
        view! {
        <main>
            <h1>"Analytics 管理員登入"</h1>
            <p>"以管理員登入碼建立 30 分鐘工作階段。來源服務 token 不能用來登入。"</p>
            <form method="post" action="/login">
                <label>"管理員帳號"
                    <input name="username" type="text" value="admin" required autocomplete="username" />
                </label>
                <label>"管理員登入碼"
                    <input name="key" type="password" required autocomplete="current-password" />
                </label>
                <button>"登入"</button>
            </form>
        </main> }
        .to_html(),
    )
}

fn admin_account_name_is_valid(value: &str) -> bool {
    value == "admin"
}

fn default_legacy_admin_name() -> String {
    "admin".into()
}

#[derive(Deserialize)]
struct Login {
    #[serde(default = "default_legacy_admin_name")]
    username: String,
    key: String,
}

async fn login(
    State(state): State<Admin>,
    headers: HeaderMap,
    Form(form): Form<Login>,
) -> Response {
    if !origin_ok(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some(config) = &state.config else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    if !admin_account_name_is_valid(&form.username) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let mut attempts = match state.attempts.lock() {
        Ok(v) => v,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };

    if attempts.0.elapsed() >= Duration::from_secs(60) {
        *attempts = (Instant::now(), 0);
    }

    if attempts.1 >= 10 {
        // 窗口已在本函式上方檢查是否超過 60 秒並重置。
        // 將剩餘時間向上取整，避免回傳 0 秒但仍拒絕登入。
        let remaining = Duration::from_secs(60).saturating_sub(attempts.0.elapsed());
        let mut seconds = remaining.as_secs();
        if remaining.subsec_nanos() > 0 {
            seconds += 1;
        }
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, seconds.max(1).to_string())],
        )
            .into_response();
    }

    // Only accept the generated 256-bit login key, not a low-entropy password hashed with SHA256.
    if form.key.len() != 64
        || !form.key.bytes().all(|c| c.is_ascii_hexdigit())
        || !same(&hash(&form.key), &config.key_hash)
    {
        attempts.1 += 1;
        return (StatusCode::UNAUTHORIZED, "登入失敗，請返回登入頁重試").into_response();
    }

    drop(attempts);

    let raw = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());

    let mut sessions = match state.sessions.lock() {
        Ok(s) => s,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    sessions.retain(|_, s| s.expires > Instant::now());
    if sessions.len() >= 1024 {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if let Some(old) = cookie_key(&headers) {
        sessions.remove(&old);
    }
    sessions.insert(
        hash(&raw),
        Session {
            expires: Instant::now() + TTL,
            csrf: Uuid::new_v4().simple().to_string(),
        },
    );

    let mut response = Redirect::to("/dashboard").into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, session_cookie(&raw));
    response
}

#[derive(Deserialize)]
struct Logout {
    csrf: String,
}

async fn logout(
    State(state): State<Admin>,
    headers: HeaderMap,
    Form(form): Form<Logout>,
) -> Response {
    if !origin_ok(&headers) || !csrf_for(&state, &headers).is_some_and(|v| same(&v, &form.csrf)) {
        return StatusCode::FORBIDDEN.into_response();
    }

    if let Some(key) = cookie_key(&headers)
        && let Ok(mut sessions) = state.sessions.lock()
    {
        sessions.remove(&key);
    }

    let mut response = Redirect::to("/login").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        "analytics_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"
            .parse()
            .unwrap(),
    );
    response
}

#[derive(Default, Deserialize)]
struct Filter {
    source: Option<String>,
    kind: Option<String>,
}

async fn rows(state: &Admin, source: &str, kind: &str) -> Result<Vec<EventSummary>, StatusCode> {
    if !matches!(source, "web_chat" | "desktop_pet")
        || (!kind.is_empty() && (!crate::valid_event_type(kind) || kind.contains('\0')))
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    let result = if kind.is_empty() {
        summary_for_source(&state.ingest.pool, source).await
    } else {
        summary_for_source_and_type(&state.ingest.pool, source, kind).await
    };
    result.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}

async fn dashboard(
    State(state): State<Admin>,
    headers: HeaderMap,
    Query(filter): Query<Filter>,
) -> Response {
    let Some(csrf) = csrf_for(&state, &headers) else {
        return Redirect::to("/login").into_response();
    };

    let source = filter.source.unwrap_or_else(|| "web_chat".into());

    let kind = filter.kind.unwrap_or_default();

    let (status, output) = match rows(&state,&source,&kind).await {
        Ok(rows) => {
            let total=crate::summary_total(&rows).map(|v| v.to_string()).unwrap_or_else(|| "聚合值異常".into());
            let empty=rows.is_empty();
            (StatusCode::OK, view! {
                <section>
                    <p>{if empty { "沒有符合條件的已提交事件" } else { "查詢完成" }}</p>
                    <p>{format!("總數：{total}")}</p>
                    <table>
                        <thead>
                            <tr>
                                <th>"來源"</th>
                                <th>"事件種類"</th>
                                <th>"數量"</th>
                            </tr>
                        </thead>
                        <tbody>{rows.into_iter().map(|r| view! {
                            <tr>
                                <td>{r.source_app}</td>
                                <td>{r.event_type}</td>
                                <td>{r.total}</td>
                            </tr> }).collect_view()}
                        </tbody>
                    </table>
                </section> }.to_html())
        }
        Err(code) => (code, view! {
            <p role="alert">{if code==StatusCode::BAD_REQUEST { "查詢條件不合法" } else { "資料庫暫時無法查詢，請重試" }}</p> 
        }.to_html())
    };

    let body = view! {
        <main>
            <h1>"Analytics 管理後台"</h1>
            <form method="get" action="/dashboard">
                <label>"來源"
                    <select name="source">
                        <option value="web_chat" selected=source=="web_chat">"Chat"</option>
                        <option value="desktop_pet" selected=source=="desktop_pet">"Pet"</option>
                    </select>
                </label>
                <label>"事件種類（空白為全部）"
                    <input name="kind" value=kind />
                </label>
                <button>"查詢"</button>
            </form>
            <div inner_html=output></div>
            <form method="post" action="/logout">
                <input type="hidden" name="csrf" value=csrf />
                <button>"登出"</button>
            </form>
        </main>
    }
    .to_html();
    (status, document(body)).into_response()
}

async fn summary(
    State(state): State<Admin>,
    Path(source): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Vec<EventSummary>>, StatusCode> {
    csrf_for(&state, &headers).ok_or(StatusCode::UNAUTHORIZED)?;
    Ok(Json(rows(&state, &source, "").await?))
}

async fn kind_summary(
    State(state): State<Admin>,
    Path((source, kind)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<Vec<EventSummary>>, StatusCode> {
    csrf_for(&state, &headers).ok_or(StatusCode::UNAUTHORIZED)?;
    Ok(Json(rows(&state, &source, &kind).await?))
}

pub fn build(ingest: IngestState, config: Option<Config>) -> Router {
    let admin = Admin {
        ingest: ingest.clone(),
        config,
        sessions: Arc::new(Mutex::new(HashMap::new())),
        attempts: Arc::new(Mutex::new((Instant::now(), 0))),
    };
    let protected = Router::new()
        .route("/", get(dashboard))
        .route("/dashboard", get(dashboard))
        .route("/api/v1/events/{source}/summary", get(summary))
        .route(
            "/api/v1/events/{source}/summary/{event_type}",
            get(kind_summary),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            admin.clone(),
            refresh_session,
        ))
        .route("/login", get(login_page).post(login))
        .route("/logout", post(logout))
        .with_state(admin);

    Router::new()
        .route("/healthz", get(crate::health))
        .route("/api/v1/events", post(crate::ingest))
        .with_state(ingest)
        .merge(protected)
        .layer(DefaultBodyLimit::max(4096))
        .layer(axum::middleware::map_response(
            |mut response: Response| async move {
                response
                    .headers_mut()
                    .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
                response
            },
        ))
}

#[cfg(test)]
mod tests {
    use crate::pg_store;

    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    fn app() -> Router {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(Duration::from_millis(100))
            .connect_lazy("postgres://fixture:fixture@127.0.0.1:1/fixture")
            .unwrap();
        build(
            IngestState {
                pool,
                tokens: Arc::new(crate::IngestTokens {
                    chat: "fixture-chat".into(),
                    pet: "fixture-pet".into(),
                }),
            },
            Some(Config::new(hash(&"a".repeat(64))).unwrap()),
        )
    }

    fn request(method: &str, url: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(url)
            .header("origin", ORIGIN)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    #[tokio::test]
    async fn admin_login_guard_and_session_are_separate_from_source_token() {
        let app = app();
        let response = app
            .clone()
            .oneshot(request("GET", "/dashboard", ""))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);

        let mut req = request("GET", "/api/v1/events/web_chat/summary", "");
        req.headers_mut().insert(
            header::AUTHORIZATION,
            "Bearer fixture-chat".parse().unwrap(),
        );

        assert_eq!(
            app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );

        assert_eq!(
            app.clone()
                .oneshot(request("POST", "/login", "key=wrong"))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );

        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/login",
                &format!("key={}", "a".repeat(64)),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);

        let cookie = response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();

        let mut req = request("POST", "/logout", "csrf=wrong");
        req.headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());

        assert_eq!(
            app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );

        let mut req = request("GET", "/dashboard", "");
        req.headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());
        let response = app.clone().oneshot(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let html = String::from_utf8(
            axum::body::to_bytes(response.into_body(), 1_000_000)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();

        let csrf = html
            .split("name=\"csrf\" value=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();

        let mut req = request("POST", "/logout", &format!("csrf={csrf}"));
        req.headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());

        assert_eq!(
            app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::SEE_OTHER
        );

        let mut req = request("GET", "/api/v1/events/web_chat/summary", "");
        req.headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());

        assert_eq!(
            app.oneshot(req).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn origin_and_expired_sessions_fail_closed() {
        let app = app();
        let mut req = request("POST", "/login", &format!("key={}", "a".repeat(64)));
        req.headers_mut().remove(header::ORIGIN);

        assert_eq!(
            app.oneshot(req).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );

        let pool = sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(Duration::from_millis(100))
            .connect_lazy("postgres://fixture:fixture@127.0.0.1:1/fixture")
            .unwrap();
        let state = Admin {
            ingest: IngestState {
                pool,
                tokens: Arc::new(crate::IngestTokens {
                    chat: "c".into(),
                    pet: "p".into(),
                }),
            },
            config: None,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            attempts: Arc::new(Mutex::new((Instant::now(), 0))),
        };

        let raw = "b".repeat(64);
        state.sessions.lock().unwrap().insert(
            hash(&raw),
            Session {
                expires: Instant::now() - Duration::from_secs(1),
                csrf: "test".into(),
            },
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("analytics_session={raw}").parse().unwrap(),
        );

        assert!(csrf_for(&state, &headers).is_none());
    }

    #[tokio::test]
    #[ignore = "requires dedicated ANALYTICS_TEST_DATABASE_URL with CREATE SCHEMA permission"]
    async fn login_query_postgres_and_logout_flow() {
        // The test creates and drops only its unique schema, never application tables.
        let url = std::env::var("DATABASE_URL").expect("dedicated test database required");

        let owner = sqlx::PgPool::connect(&url).await.unwrap();

        let schema = format!("day16_{}", Uuid::new_v4().simple());
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&owner)
            .await
            .unwrap();

        let search = schema.clone();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .after_connect(move |conn, _| {
                let sql = format!("SET search_path TO {search}");
                Box::pin(async move {
                    sqlx::query(sqlx::AssertSqlSafe(sql)).execute(conn).await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();

        sqlx::query(include_str!(
            "../../migrations/20261014223748_create_product_events.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let event = crate::EventInput {
            schema_version: 1,
            event_id: Uuid::new_v4(),
            source_app: "web_chat".into(),
            event_type: "chat.message.persisted".into(),
        };

        assert_eq!(
            pg_store::persist_event(&pool, &event).await.unwrap(),
            pg_store::PersistOutcome::Stored
        );

        assert_eq!(
            pg_store::persist_event(&pool, &event).await.unwrap(),
            pg_store::PersistOutcome::AlreadyExists
        );

        let app = build(
            IngestState {
                pool: pool.clone(),
                tokens: Arc::new(crate::IngestTokens {
                    chat: "fixture-chat".into(),
                    pet: "fixture-pet".into(),
                }),
            },
            Some(Config::new(hash(&"a".repeat(64))).unwrap()),
        );

        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/login",
                &format!("key={}", "a".repeat(64)),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);

        let cookie = response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();

        let mut req = request(
            "GET",
            "/dashboard?source=web_chat&kind=chat.message.persisted",
            "",
        );
        req.headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());
        let response = app.clone().oneshot(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let html = String::from_utf8(
            axum::body::to_bytes(response.into_body(), 1_000_000)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();

        assert!(html.contains("chat.message.persisted"));

        assert!(html.contains("總數：1"));

        let csrf = html
            .split("name=\"csrf\" value=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();

        let mut req = request("POST", "/logout", &format!("csrf={csrf}"));
        req.headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());

        assert_eq!(
            app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::SEE_OTHER
        );

        let mut req = request("GET", "/api/v1/events/web_chat/summary", "");
        req.headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());

        assert_eq!(
            app.oneshot(req).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );

        pool.close().await;
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
            .execute(&owner)
            .await
            .unwrap();
        owner.close().await;
    }

    #[test]
    fn day18_activity_extends_valid_session_only() {
        let now = Instant::now();
        let mut session = Session {
            expires: now + Duration::from_secs(5),
            csrf: "fixture".into(),
        };
        touch_session(&mut session, now + Duration::from_secs(2));
        assert_eq!(session.expires, now + Duration::from_secs(2) + TTL);
    }

    #[tokio::test]
    async fn day19_successful_logins_do_not_consume_failure_budget() {
        let router = app();
        for attempt in 1..=11 {
            let response = router
                .clone()
                .oneshot(request(
                    "POST",
                    "/login",
                    &format!("key={}", "a".repeat(64)),
                ))
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::SEE_OTHER,
                "success {attempt}"
            );
            assert_eq!(response.headers()[header::LOCATION], "/dashboard");
            let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
            for attribute in [
                "analytics_session=",
                "HttpOnly",
                "SameSite=Strict",
                "Path=/",
                "Max-Age=1800",
            ] {
                assert!(cookie.contains(attribute), "missing {attribute}");
            }
        }
        for attempt in 1..=10 {
            let response = router
                .clone()
                .oneshot(request(
                    "POST",
                    "/login",
                    &format!("key={}", "b".repeat(64)),
                ))
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "failure {attempt}"
            );
            assert!(!response.headers().contains_key(header::SET_COOKIE));
        }
        for key in ["b".repeat(64), "a".repeat(64)] {
            let response = router
                .clone()
                .oneshot(request("POST", "/login", &format!("key={key}")))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
            assert!(!response.headers().contains_key(header::SET_COOKIE));
        }
    }

    #[tokio::test]
    async fn day19_success_does_not_reset_previous_failures() {
        let router = app();
        for _ in 0..9 {
            assert_eq!(
                router
                    .clone()
                    .oneshot(request("POST", "/login", "key=wrong"))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            router
                .clone()
                .oneshot(request(
                    "POST",
                    "/login",
                    &format!("key={}", "a".repeat(64))
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            router
                .clone()
                .oneshot(request("POST", "/login", "key=wrong"))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            router
                .oneshot(request("POST", "/login", "key=wrong"))
                .await
                .unwrap()
                .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[tokio::test]
    async fn day20_login_rate_limit_provides_retry_after() {
        let router = app();
        for _ in 0..10 {
            let response = router
                .clone()
                .oneshot(request("POST", "/login", "key=wrong"))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        let response = router
            .oneshot(request("POST", "/login", "key=wrong"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let seconds: u64 = response.headers()[header::RETRY_AFTER]
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!((1..=60).contains(&seconds));
        assert!(!response.headers().contains_key(header::SET_COOKIE));
    }

    #[test]
    fn day21_admin_account_is_explicit() {
        assert!(super::admin_account_name_is_valid("admin"));
        assert!(!super::admin_account_name_is_valid("guest"));
        assert!(!super::admin_account_name_is_valid("ADMIN"));
    }
}
