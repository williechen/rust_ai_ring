pub mod admin;
pub mod bootstrap;
mod csv;
mod dashboard_summary;
pub mod dashboard_ui;
mod dashboard_url;
pub mod export;
pub mod password_login;
pub mod pg_store;
mod session_cookie;

use analytics_persistence::summary_for_source;
use analytics_persistence::{EventSummary, summary_for_source_and_type};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    http::{HeaderMap, header::AUTHORIZATION},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::pg_store::{PersistError, PersistOutcome};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventInput {
    pub schema_version: u16,
    pub event_id: Uuid,
    pub source_app: String,
    pub event_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventIdentity {
    source_app: String,
    event_id: String,
}

impl EventIdentity {
    pub fn new(source_app: impl Into<String>, event_id: impl Into<String>) -> Self {
        Self {
            source_app: source_app.into(),
            event_id: event_id.into(),
        }
    }
}

#[derive(Default)]
pub struct InMemoryDeduplicator {
    seen: std::collections::HashSet<EventIdentity>,
}

impl InMemoryDeduplicator {
    pub fn accept_once(&mut self, identity: EventIdentity) -> bool {
        self.seen.insert(identity)
    }
    /// 無效事件不占用去重鍵；Some(false) 代表本程序已看過。
    /// 僅用於記憶體契約學習，不是資料庫提交收據。
    pub fn accept_valid_event(&mut self, event: &EventInput) -> Option<bool> {
        if !valid_event(event) {
            return None;
        }
        Some(self.accept_once(EventIdentity::new(
            event.source_app.clone(),
            event.event_id.to_string(),
        )))
    }
}

// 事件類型長度以 UTF-8 位元組計算，避免儲存層限制不一致。
pub fn valid_event_type(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.trim() == value
}

// 只做事件資料驗證；憑證、持久化與 ACK 各由真正 Adapter 負責。
pub fn valid_event(event: &EventInput) -> bool {
    event.schema_version == 1
        && !event.event_id.is_nil()
        && matches!(event.source_app.as_str(), "web_chat" | "desktop_pet")
        && valid_event_type(event.event_type.as_str())
}

fn single_authorization(headers: &HeaderMap) -> Option<&str> {
    let mut values = headers.get_all(AUTHORIZATION).iter();
    let first = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    Some(first)
}

#[derive(Clone)]
pub struct IngestTokens {
    pub chat: String,
    pub pet: String,
}

#[derive(Clone)]
pub struct IngestState {
    pub pool: PgPool,
    pub tokens: Arc<IngestTokens>,
}

impl IngestTokens {
    fn authorized(&self, source: &str, header: &str) -> bool {
        let token = match source {
            "web_chat" => &self.chat,
            "desktop_pet" => &self.pet,
            _ => return false,
        };
        // 不接受空憑證；固定長度逐位比較避免提早依內容退出。
        if token.is_empty() || token.len() > 512 {
            return false;
        }
        let expected = format!("Bearer {token}");
        let a = expected.as_bytes();
        let b = header.as_bytes();
        let mut diff = a.len() ^ b.len();
        for i in 0..a.len().max(b.len()) {
            diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
        }
        diff == 0
    }
}

fn outcome_status(value: PersistOutcome) -> StatusCode {
    match value {
        PersistOutcome::Stored => StatusCode::CREATED,
        PersistOutcome::AlreadyExists => StatusCode::OK,
        PersistOutcome::IdentityConflict => StatusCode::CONFLICT,
    }
}

async fn health() -> StatusCode {
    StatusCode::OK
}

pub async fn ingest(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Json(event): Json<EventInput>,
) -> StatusCode {
    if !valid_event(&event) {
        return StatusCode::BAD_REQUEST;
    }
    let authorization = single_authorization(&headers).unwrap_or("");
    if !state.tokens.authorized(&event.source_app, authorization) {
        return StatusCode::UNAUTHORIZED;
    }
    match pg_store::persist_event(&state.pool, &event).await {
        Ok(outcome) => outcome_status(outcome),
        Err(PersistError::Invalid) => StatusCode::BAD_REQUEST,
        Err(PersistError::Database(_)) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn summary_total(rows: &[EventSummary]) -> Option<i64> {
    rows.iter().try_fold(0_i64, |sum, row| {
        if row.total < 0 {
            None
        } else {
            sum.checked_add(row.total)
        }
    })
}

pub fn build_app(state: IngestState) -> Router {
    password_login::build(state)
}

#[cfg(test)]
async fn source_summary(
    State(state): State<IngestState>,
    axum::extract::Path(source): axum::extract::Path<String>,
    headers: HeaderMap,
) -> Result<Json<Vec<EventSummary>>, StatusCode> {
    if !matches!(source.as_str(), "web_chat" | "desktop_pet") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    // URL 來源必須與憑證相符；不接受另一來源的 token。
    if !state.tokens.authorized(&source, authorization) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let rows = summary_for_source(&state.pool, &source)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(rows))
}

async fn source_kind_summary(
    State(state): State<IngestState>,
    axum::extract::Path((source, event_type)): axum::extract::Path<(String, String)>,
    headers: HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    // 先驗人員 Session，再驗來源／事件類型，最後才碰統計資料。
    let raw = password_login::require_query_session(&state, &headers).await?;
    if !matches!(source.as_str(), "web_chat" | "desktop_pet") {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !valid_event_type(&event_type) || event_type.contains('\0') {
        return Err(StatusCode::BAD_REQUEST);
    }
    let rows = summary_for_source_and_type(&state.pool, &source, &event_type)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(password_login::renewed(Json(rows), &raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_store::PersistOutcome::*;

    #[test]
    fn nil_event_id_is_not_accepted() {
        let invalid = EventInput {
            schema_version: 1,
            event_id: Uuid::nil(),
            source_app: "web_chat".into(),
            event_type: "chat.message.persisted".into(),
        };
        assert!(!valid_event(&invalid));
        let valid = EventInput {
            event_id: Uuid::new_v4(),
            ..invalid
        };
        assert!(valid_event(&valid));
    }

    #[test]
    fn rejects_bad_source_version_and_event_type() {
        let mut input = EventInput {
            schema_version: 1,
            event_id: Uuid::new_v4(),
            source_app: "web_chat".into(),
            event_type: "chat.message.persisted".into(),
        };
        assert!(valid_event(&input));
        input.source_app = "invalid_source".into();
        assert!(!valid_event(&input));
        input.source_app = "desktop_pet".into();
        input.schema_version = 2;
        assert!(!valid_event(&input));
        input.schema_version = 1;
        input.event_type = "   ".into();
        assert!(!valid_event(&input));
    }

    #[test]
    fn event_type_rejects_noncanonical_boundaries() {
        assert!(valid_event_type("chat.message.persisted"));
        assert!(!valid_event_type(" chat.message.persisted"));
        assert!(!valid_event_type("chat.message.persisted "));
        assert!(!valid_event_type("\n"));
        assert!(!valid_event_type(""));
        assert!(!valid_event_type(&"x".repeat(129)));
    }

    #[test]
    fn existing_event_validation_rejects_whitespace() {
        let event = EventInput {
            schema_version: 1,
            event_id: Uuid::new_v4(),
            source_app: "web_chat".into(),
            event_type: " chat.message.persisted".into(),
        };
        assert!(!valid_event(&event));
    }

    #[test]
    fn valid_event_is_accepted_once_per_source_and_id() {
        let id = Uuid::new_v4();
        let event = EventInput {
            schema_version: 1,
            event_id: id,
            source_app: "web_chat".into(),
            event_type: "chat.message.persisted".into(),
        };
        let mut dedup = InMemoryDeduplicator::default();
        assert_eq!(dedup.accept_valid_event(&event), Some(true));
        assert_eq!(dedup.accept_valid_event(&event), Some(false));
        let other_source = EventInput {
            source_app: "desktop_pet".into(),
            event_type: "pet.cleanup.succeeded".into(),
            ..event
        };
        assert_eq!(dedup.accept_valid_event(&other_source), Some(true));
    }

    #[test]
    fn invalid_event_does_not_reserve_dedup_key() {
        let id = Uuid::new_v4();
        let mut event = EventInput {
            schema_version: 2,
            event_id: id,
            source_app: "web_chat".into(),
            event_type: "chat.message.persisted".into(),
        };
        let mut dedup = InMemoryDeduplicator::default();
        assert_eq!(dedup.accept_valid_event(&event), None);
        event.schema_version = 1;
        assert_eq!(dedup.accept_valid_event(&event), Some(true));
    }

    #[test]
    fn token_must_match_the_claimed_source() {
        let tokens = IngestTokens {
            chat: "chat-secret".into(),
            pet: "pet-secret".into(),
        };
        assert!(tokens.authorized("web_chat", "Bearer chat-secret"));
        assert!(tokens.authorized("desktop_pet", "Bearer pet-secret"));
        assert!(!tokens.authorized("desktop_pet", "Bearer chat-secret"));
        assert!(!tokens.authorized("web_chat", "Bearer "));
        assert!(!tokens.authorized("web_chat", "chat-secret"));
    }

    #[test]
    fn persist_outcomes_have_distinct_http_codes() {
        assert_eq!(outcome_status(Stored), StatusCode::CREATED);
        assert_eq!(outcome_status(AlreadyExists), StatusCode::OK);
        assert_eq!(outcome_status(IdentityConflict), StatusCode::CONFLICT);
    }
}

#[cfg(test)]
mod source_summary_tests {
    use super::*;
    use axum::extract::Path;

    fn fixture_tokens() -> Arc<IngestTokens> {
        Arc::new(IngestTokens {
            chat: "fixture-chat-only".into(),
            pet: "fixture-pet-only".into(),
        })
    }
    fn headers(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, format!("Bearer {token}").parse().unwrap());
        headers
    }

    #[tokio::test]
    async fn summary_rejects_cross_source_before_database_access() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://fixture:fixture@127.0.0.1:1/unused")
            .unwrap();
        pool.close().await;
        let state = IngestState {
            pool,
            tokens: fixture_tokens(),
        };
        for (source, headers, expected) in [
            (
                "desktop_pet",
                headers("fixture-chat-only"),
                StatusCode::UNAUTHORIZED,
            ),
            ("web_chat", HeaderMap::new(), StatusCode::UNAUTHORIZED),
            (
                "unknown",
                headers("fixture-chat-only"),
                StatusCode::BAD_REQUEST,
            ),
            (
                "web_chat",
                headers("fixture-chat-only"),
                StatusCode::SERVICE_UNAVAILABLE,
            ),
        ] {
            let result = source_summary(State(state.clone()), Path(source.into()), headers).await;
            assert_eq!(result.err().unwrap(), expected);
        }
    }

    #[cfg(feature = "postgres-integration")]
    #[sqlx::test(migrations = "../migrations")]
    async fn summary_counts_committed_rows_per_source_and_kind(pool: PgPool) {
        let id = Uuid::new_v4();
        for (source, event_id, kind) in [
            ("web_chat", id, "chat.message.persisted"),
            ("web_chat", Uuid::new_v4(), "chat.message.persisted"),
            ("web_chat", Uuid::new_v4(), "chat.room.created"),
            ("desktop_pet", id, "pet.cleanup.succeeded"),
        ] {
            let event = EventInput {
                schema_version: 1,
                event_id,
                source_app: source.into(),
                event_type: kind.into(),
            };
            assert_eq!(
                pg_store::persist_event(&pool, &event).await.unwrap(),
                PersistOutcome::Stored
            );
            assert_eq!(
                pg_store::persist_event(&pool, &event).await.unwrap(),
                PersistOutcome::AlreadyExists
            );
        }
        let state = IngestState {
            pool: pool.clone(),
            tokens: fixture_tokens(),
        };
        let Json(chat) = source_summary(
            State(state.clone()),
            Path("web_chat".into()),
            headers("fixture-chat-only"),
        )
        .await
        .unwrap();
        assert_eq!(chat.len(), 2);
        assert_eq!(chat[0].event_type, "chat.message.persisted");
        assert_eq!(chat[0].total, 2);
        assert_eq!(chat[1].event_type, "chat.room.created");
        assert_eq!(chat[1].total, 1);
        assert!(chat.iter().all(|v| v.source_app == "web_chat"));
        let Json(pet) = source_summary(
            State(state),
            Path("desktop_pet".into()),
            headers("fixture-pet-only"),
        )
        .await
        .unwrap();
        assert_eq!(pet.len(), 1);
        assert_eq!(pet[0].source_app, "desktop_pet");
        assert_eq!(pet[0].total, 1);

        // 查詢不依賴 event_id 儲存型別；既有 migration 保持不變。
        sqlx::query(
            "ALTER TABLE product_events ALTER COLUMN event_id TYPE UUID USING event_id::uuid",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(summary_for_source(&pool, "web_chat").await.unwrap(), chat);
    }

    #[cfg(feature = "postgres-integration")]
    #[sqlx::test(migrations = "../migrations")]
    async fn empty_and_rolled_back_events_are_not_counted(pool: PgPool) {
        assert!(
            summary_for_source(&pool, "web_chat")
                .await
                .unwrap()
                .is_empty()
        );
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO product_events(source_app,event_id,schema_version,event_type)
             VALUES ('web_chat',$1,1,'chat.message.persisted')",
        )
        .bind(Uuid::new_v4().to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.rollback().await.unwrap();
        assert!(
            summary_for_source(&pool, "web_chat")
                .await
                .unwrap()
                .is_empty()
        );
    }
}

#[cfg(test)]
mod source_kind_summary_tests {
    use super::*;
    use axum::extract::Path;

    fn state(pool: PgPool) -> IngestState {
        IngestState {
            pool,
            tokens: Arc::new(IngestTokens {
                chat: "fixture-chat-only".into(),
                pet: "fixture-pet-only".into(),
            }),
        }
    }

    fn headers(token: &str) -> HeaderMap {
        let mut value = HeaderMap::new();
        value.insert(AUTHORIZATION, format!("Bearer {token}").parse().unwrap());
        value
    }

    #[tokio::test]
    async fn day36_source_token_alone_cannot_read_even_before_database_access() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://fixture:fixture@127.0.0.1:1/unused")
            .unwrap();
        pool.close().await;
        let state = state(pool);
        for (source, kind) in [
            ("web_chat", "chat.message.persisted"),
            ("desktop_pet", "chat.message.persisted"),
            ("unknown", "chat.message.persisted"),
            ("web_chat", " "),
        ] {
            let result = source_kind_summary(
                State(state.clone()),
                Path((source.into(), kind.into())),
                headers("fixture-chat-only"),
            )
            .await;
            assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
        }
    }

    #[cfg(feature = "postgres-integration")]
    #[sqlx::test(migrations = "../migrations")]
    async fn filters_committed_deduplicated_rows_by_source_and_kind(pool: PgPool) {
        let shared_id = Uuid::new_v4();
        for (source, id, kind) in [
            ("web_chat", shared_id, "chat.message.persisted"),
            ("web_chat", Uuid::new_v4(), "chat.message.persisted"),
            ("web_chat", Uuid::new_v4(), "chat.room.created"),
            ("desktop_pet", shared_id, "chat.message.persisted"),
        ] {
            let event = EventInput {
                schema_version: 1,
                event_id: id,
                source_app: source.into(),
                event_type: kind.into(),
            };
            assert_eq!(
                pg_store::persist_event(&pool, &event).await.unwrap(),
                PersistOutcome::Stored
            );
            assert_eq!(
                pg_store::persist_event(&pool, &event).await.unwrap(),
                PersistOutcome::AlreadyExists
            );
        }
        let rows = analytics_persistence::summary_for_source_and_type(
            &pool,
            "web_chat",
            "chat.message.persisted",
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source_app, "web_chat");
        assert_eq!(rows[0].event_type, "chat.message.persisted");
        assert_eq!(rows[0].total, 2);
        // SQL 字元視為資料，不變成跨來源的查詢條件。
        assert!(
            analytics_persistence::summary_for_source_and_type(&pool, "web_chat", "' OR TRUE --")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            analytics_persistence::summary_for_source_and_type(&pool, "web_chat", "not.recorded")
                .await
                .unwrap()
                .is_empty()
        );
        // 原 Day 13 的未篩選查詢仍可使用。
        assert_eq!(
            summary_for_source(&pool, "web_chat").await.unwrap().len(),
            2
        );

        let mut tx = pool.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO product_events(source_app,event_id,schema_version,event_type)
             VALUES ('web_chat',$1,1,'chat.message.persisted')",
        )
        .bind(Uuid::new_v4().to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(
            analytics_persistence::summary_for_source_and_type(
                &pool,
                "web_chat",
                "chat.message.persisted"
            )
            .await
            .unwrap()[0]
                .total,
            2
        );

        sqlx::query(
            "ALTER TABLE product_events ALTER COLUMN event_id TYPE UUID USING event_id::uuid",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            analytics_persistence::summary_for_source_and_type(
                &pool,
                "web_chat",
                "chat.message.persisted"
            )
            .await
            .unwrap(),
            rows
        );
    }
}

#[cfg(test)]
mod day16_total_tests {

    use super::summary_total;
    use analytics_persistence::EventSummary;

    #[test]
    fn total_uses_actual_committed_summary_counts() {
        let rows = vec![
            EventSummary {
                source_app: "web_chat".to_owned(),
                event_type: "chat.message.persisted".to_owned(),
                total: 2,
            },
            EventSummary {
                source_app: "web_chat".to_owned(),
                event_type: "chat.room.created".to_owned(),
                total: 3,
            },
        ];
        assert_eq!(summary_total(&rows), Some(5));
    }

    #[cfg(test)]
    mod day28_dashboard_leptos_tests {
        #[test]
        fn day28_leptos_dashboard_renders_source_and_links() {
            let html = crate::dashboard_ui::render_dashboard("Pet", 2);
            assert!(html.contains("全部事件類型：2 筆已提交事件"));
            assert!(html.contains("/dashboard?source=web_chat"));
            assert!(html.contains("/dashboard?source=desktop_pet"));
            assert!(html.contains("修改密碼"));
            assert!(!html.contains("test-pet"));
        }
    }
}

#[cfg(feature = "postgres-integration")]
#[sqlx::test(migrations = "../migrations")]
async fn day36_live_router_requires_person_session_not_source_token(pool: PgPool) {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, header},
    };
    use sha2::{Digest, Sha256};
    use tower::ServiceExt;

    let digest = |s: &str| {
        Sha256::digest(s.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let user_id = crate::bootstrap::ensure_default(&pool).await.unwrap();
    // 只在隔離測試 DB 建立「已完成首次改密碼」的 Session fixture；
    // 正式修改密碼流程另由 day27/day34 HTTP 測試覆蓋。
    sqlx::query("UPDATE uac_user SET is_init_pd=FALSE WHERE id=$1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    let password_hash: String = sqlx::query_scalar("SELECT user_pd FROM uac_user WHERE id=$1")
        .bind(&user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let raw = "a".repeat(64);
    sqlx::query(
        "INSERT INTO uac_login_session
         (token_hash,user_id,credential_fingerprint,restricted,expires_at)
         VALUES ($1,$2,$3,FALSE,floor(extract(epoch from now()))::bigint+1800)",
    )
    .bind(digest(&raw))
    .bind(&user_id)
    .bind(digest(&password_hash))
    .execute(&pool)
    .await
    .unwrap();

    let event = EventInput {
        schema_version: 1,
        event_id: Uuid::new_v4(),
        source_app: "web_chat".into(),
        event_type: "chat.message.persisted".into(),
    };
    assert_eq!(
        pg_store::persist_event(&pool, &event).await.unwrap(),
        pg_store::PersistOutcome::Stored
    );
    let app = crate::build_app(IngestState {
        pool: pool.clone(),
        tokens: Arc::new(IngestTokens {
            chat: "fixture-chat-only".into(),
            pet: "fixture-pet-only".into(),
        }),
    });
    let path = "/api/v1/events/web_chat/summary/chat.message.persisted";
    for token in ["fixture-chat-only", "fixture-pet-only"] {
        let denied = app
            .clone()
            .oneshot(
                Request::get(path)
                    .header(AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    }
    let cookie = format!("analytics_session={raw}");
    let ok = app
        .clone()
        .oneshot(
            Request::get(path)
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    assert!(ok.headers().contains_key(header::SET_COOKIE)); // 活動續期
    let body = to_bytes(ok.into_body(), 8192).await.unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["source_app"], "web_chat");
    assert_eq!(rows[0]["event_type"], "chat.message.persisted");
    assert_eq!(rows[0]["total"], 1);

    // 同一已登入人員仍需通過事件類型校驗。
    let bad = app
        .clone()
        .oneshot(
            Request::get("/api/v1/events/web_chat/summary/%20")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

    // 登出/密碼變更/停用都由相同 Session adapter 拒絕。
    sqlx::query("DELETE FROM uac_login_session WHERE token_hash=$1")
        .bind(digest(&raw))
        .execute(&pool)
        .await
        .unwrap();
    let revoked = app
        .oneshot(
            Request::get(path)
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
}
