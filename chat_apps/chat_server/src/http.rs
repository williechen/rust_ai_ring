use crate::state::AppState;
use axum::Extension;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::{Json, http::StatusCode};
use serde::Serialize;
use shared::{CreateRoomRequest, CreateRoomResponse};
use sqlx::SqlitePool;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
}

pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: state.service_name,
    })
}

pub async fn create_room(
    State(state): State<AppState>,
    Json(request): Json<CreateRoomRequest>,
) -> Result<(StatusCode, Json<CreateRoomResponse>), StatusCode> {
    if chat_application::validate_room_name(&request.name).is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let room = chat_application::create_room(
        state.room_repository.as_ref(),
        chat_application::CreateRoom {
            id: uuid::Uuid::new_v4().to_string(),
            category_room_id: "default-category".to_string(),
            name: request.name,
        },
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // Repository 成功才回 201；JSON 契約仍只含既有 id。
    Ok((
        StatusCode::CREATED,
        Json(CreateRoomResponse { id: room.id }),
    ))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RoomDetails {
    pub id: String,
    pub category_room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_label: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_count: Option<usize>,
}

#[derive(Default, serde::Deserialize)]
pub struct RoomSearch {
    #[serde(default)]
    q: String,
    #[serde(default)]
    labels: bool,
    #[serde(default)]
    popular: bool,
    #[serde(default)]
    category: String,
}

pub async fn get_room(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<RoomDetails>, StatusCode> {
    // 沿用既有 ID 契約，不要求歷史 ID 必須是 UUID。
    if id.trim().is_empty() || id.len() > 40 || id.contains('\0') {
        return Err(StatusCode::BAD_REQUEST);
    }
    let room = state
        .room_repository
        .find_by_id(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    // Adapter 回傳錯誤房間時不洩漏該房間資料。
    if room.id != id {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    Ok(Json(RoomDetails {
        id: room.id,
        category_room_id: room.category_room_id,
        category_label: None,
        name: room.name,
        online_count: None,
    }))
}

pub async fn list_rooms(
    State(state): State<AppState>,
    Query(search): Query<RoomSearch>,
) -> Result<Json<Vec<RoomDetails>>, StatusCode> {
    if search.q.len() > 200 || search.q.contains('\0') || search.popular && !search.q.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if search.popular {
        let rooms = chat_application::room_directory::popular(
            state.room_repository.as_ref(),
            &state.room_hub,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(Json(
            rooms
                .into_iter()
                .map(|ranked| RoomDetails {
                    id: ranked.room.id,
                    category_room_id: ranked.room.category_room_id,
                    category_label: ranked.room.category_label.filter(|_| search.labels),
                    name: ranked.room.name,
                    online_count: Some(ranked.online_count),
                })
                .collect(),
        ));
    }
    let rooms = chat_application::room_directory::filtered(
        state.room_repository.as_ref(),
        &search.q,
        &search.category,
    )
    .await
    .map_err(|error| match error {
        chat_application::room_directory::DirectoryError::InvalidQuery => StatusCode::BAD_REQUEST,
        chat_application::room_directory::DirectoryError::Store(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;
    Ok(Json(
        rooms
            .into_iter()
            .map(|room| RoomDetails {
                id: room.id,
                category_room_id: room.category_room_id,
                category_label: room.category_label.filter(|_| search.labels),
                name: room.name,
                online_count: None,
            })
            .collect(),
    ))
}

/// 歷史讀取須在每次 HTTP 請求重新驗證會員與房間排除狀態。
pub async fn room_history(
    State(state): State<AppState>,
    Extension(pool): Extension<SqlitePool>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Vec<chat_domain::ChatMessage>>, StatusCode> {
    if id.trim().is_empty() || id.len() > 40 || id.contains('\0') {
        return Err(StatusCode::BAD_REQUEST);
    }
    let member = crate::auth_session::active_member(&pool, &headers).await?;
    if crate::member_self::initial(&pool, &member).await? {
        return Err(StatusCode::FORBIDDEN);
    }
    let room = state
        .room_repository
        .find_by_id(&id)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !room.is_some_and(|room| room.id == id) {
        return Err(StatusCode::NOT_FOUND);
    }
    if chat_persistence::room_access::is_excluded(&pool, &member, &id)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(Json(
        state
            .message_repository
            .list_by_room(&id)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?,
    ))
}

#[cfg(test)]
mod room_http_tests {
    use super::*;
    use axum::response::IntoResponse;
    use std::sync::Arc;

    pub(super) async fn fixture() -> (AppState, sqlx::SqlitePool) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        chat_persistence::migrate(&pool).await.unwrap();
        let state = AppState::new(
            leptos_config::get_configuration(None)
                .unwrap()
                .leptos_options,
            Arc::new(chat_persistence::SqliteRoomRepository::new(pool.clone())),
            Arc::new(chat_persistence::SqliteMessageRepository::new(pool.clone())),
        );
        (state, pool)
    }

    #[tokio::test]
    async fn invalid_room_names_never_write() {
        let (state, pool) = fixture().await;
        for name in [
            "".to_string(),
            " \n\t".into(),
            "a\0b".into(),
            "x".repeat(201),
            "房".repeat(67),
        ] {
            let response = create_room(State(state.clone()), Json(CreateRoomRequest { name }))
                .await
                .into_response();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        pool.close().await;
    }

    #[tokio::test]
    async fn valid_room_returns_201_and_saved_id() {
        let (state, pool) = fixture().await;
        let name = " 房間 ".to_string();
        let response = create_room(State(state), Json(CreateRoomRequest { name: name.clone() }))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let created: CreateRoomResponse = serde_json::from_slice(&body).unwrap();
        assert!(uuid::Uuid::parse_str(&created.id).is_ok());
        let row: (String, String) =
            sqlx::query_as("SELECT name, category_room_id FROM rooms WHERE id = ?1")
                .bind(created.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        // 保留原名稱；trim 只檢查是否全為空白。
        assert_eq!(row.0, name);
        assert_eq!(row.1, "default-category");
        pool.close().await;
    }

    #[tokio::test]
    async fn database_failure_is_not_created() {
        let (state, pool) = fixture().await;
        pool.close().await;
        let response = create_room(
            State(state.clone()),
            Json(CreateRoomRequest {
                name: "有效名稱".into(),
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        // 即使 DB 關閉，無效輸入仍應先被拒絕。
        let response = create_room(State(state), Json(CreateRoomRequest { name: " ".into() }))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn day35_json_create_rejects_control_names_before_sqlite() {
        let (state, pool) = fixture().await;
        for name in ["房\n間", "房\t間", "房\r間", "\u{7f}"] {
            let response = create_room(
                State(state.clone()),
                Json(CreateRoomRequest { name: name.into() }),
            )
            .await
            .into_response();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM rooms")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        pool.close().await;
    }
}

#[cfg(test)]
mod room_details_tests {
    use super::*;
    use axum::{extract::Path, response::IntoResponse};
    use std::sync::Arc;

    async fn fixture() -> (AppState, sqlx::SqlitePool) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        chat_persistence::migrate(&pool).await.unwrap();
        sqlx::query("INSERT INTO codemap(category,code,value) VALUES ('room_category','category-14','測試分類')")
            .execute(&pool).await.unwrap();
        let rooms = Arc::new(chat_persistence::SqliteRoomRepository::new(pool.clone()));
        chat_application::create_room(
            rooms.as_ref(),
            chat_application::CreateRoom {
                id: "room-14".into(),
                category_room_id: "category-14".into(),
                name: "第十四天房間".into(),
            },
        )
        .await
        .unwrap();
        let state = AppState::new(
            leptos_config::get_configuration(None)
                .unwrap()
                .leptos_options,
            rooms,
            Arc::new(chat_persistence::SqliteMessageRepository::new(pool.clone())),
        );
        (state, pool)
    }

    #[tokio::test]
    async fn returns_exact_saved_room_without_writing() {
        let (state, pool) = fixture().await;
        let response = get_room(State(state.clone()), Path("room-14".into()))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let room: RoomDetails = serde_json::from_slice(&body).unwrap();
        assert_eq!(room.id, "room-14");
        assert_eq!(room.category_room_id, "category-14");
        assert_eq!(room.name, "第十四天房間");
        assert_eq!(
            get_room(State(state), Path("absent".into()))
                .await
                .unwrap_err(),
            StatusCode::NOT_FOUND
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
        pool.close().await;
    }

    #[tokio::test]
    async fn list_rooms_returns_persisted_room() {
        let (state, pool) = fixture().await;
        let response = list_rooms(State(state), Query(RoomSearch::default()))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        let rooms: Vec<RoomDetails> = serde_json::from_slice(&body).unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].id, "room-14");
        pool.close().await;
    }

    #[tokio::test]
    async fn invalid_input_and_database_failure_are_distinct() {
        let (state, pool) = fixture().await;
        pool.close().await;
        for id in [" ".to_string(), "x".repeat(41), "a\0b".into()] {
            assert_eq!(
                get_room(State(state.clone()), Path(id)).await.unwrap_err(),
                StatusCode::BAD_REQUEST
            );
        }
        assert_eq!(
            get_room(State(state), Path("room-14".into()))
                .await
                .unwrap_err(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[cfg(test)]
mod day64_category_tests {
    use super::*;
    #[tokio::test]
    async fn directory_filters_by_stable_code_even_when_labels_are_identical() {
        let (state, pool) = super::room_http_tests::fixture().await;
        for code in ["category-a", "category-b"] {
            sqlx::query(
                "INSERT INTO codemap(category,code,value) VALUES('room_category',?,'同名分類')",
            )
            .bind(code)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO rooms(id,category_room_id,name,desc,created_at,updated_at) VALUES(?,?,?,'',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").bind(code).bind(code).bind(format!("房間{code}")).execute(&pool).await.unwrap();
        }
        let search: RoomSearch = serde_json::from_value(
            serde_json::json!({"q":"","labels":true,"category":"category-a"}),
        )
        .unwrap();
        let Json(rows) = list_rooms(State(state), Query(search)).await.unwrap();
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["category-a"]
        );
    }
}

#[derive(serde::Serialize)]
pub struct CategoryDetails {
    code: String,
    label: String,
}

pub async fn room_categories(
    State(state): State<AppState>,
) -> Result<Json<Vec<CategoryDetails>>, StatusCode> {
    chat_application::room_directory::categories(state.room_repository.as_ref())
        .await
        .map(|rows| {
            Json(
                rows.into_iter()
                    .map(|c| CategoryDetails {
                        code: c.code,
                        label: c.label,
                    })
                    .collect(),
            )
        })
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}

#[cfg(test)]
mod popular_http_tests {
    use super::*;
    #[tokio::test]
    async fn popular_mode_is_limited_counted_and_preserves_legacy_json() {
        let (state, pool) = super::room_http_tests::fixture().await;
        for i in 0..12 {
            sqlx::query("INSERT INTO rooms(id,category_room_id,name,desc,created_at,updated_at) VALUES(?1,'default-category',?2,'',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)")
                .bind(format!("r{i:02}")).bind(format!("房{i:02}")).execute(&pool).await.unwrap();
        }
        let _a = state.room_hub.member_joined("r11".into(), "a".into());
        let _a2 = state.room_hub.member_joined("r11".into(), "a".into());
        let _b = state.room_hub.member_joined("r11".into(), "b".into());
        let popular = list_rooms(
            State(state.clone()),
            Query(RoomSearch {
                popular: true,
                ..Default::default()
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(popular.len(), 10);
        assert_eq!(popular[0].id, "r11");
        assert_eq!(popular[0].online_count, Some(2));
        let legacy = list_rooms(State(state.clone()), Query(RoomSearch::default()))
            .await
            .unwrap()
            .0;
        assert_eq!(legacy.len(), 12);
        assert!(
            !serde_json::to_string(&legacy)
                .unwrap()
                .contains("online_count")
        );
        assert_eq!(
            list_rooms(
                State(state.clone()),
                Query(RoomSearch {
                    popular: true,
                    q: "x".into(),
                    ..Default::default()
                })
            )
            .await
            .unwrap_err(),
            StatusCode::BAD_REQUEST
        );
        pool.close().await;
        assert_eq!(
            list_rooms(
                State(state),
                Query(RoomSearch {
                    popular: true,
                    ..Default::default()
                })
            )
            .await
            .unwrap_err(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

/// Only an authorized room member can read this room's current online display names.
pub async fn room_members(
    State(state): State<AppState>,
    Extension(pool): Extension<SqlitePool>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<
    (
        [(axum::http::header::HeaderName, &'static str); 1],
        Json<Vec<String>>,
    ),
    StatusCode,
> {
    if id.trim().is_empty() || id.len() > 40 || id.contains('\0') {
        return Err(StatusCode::BAD_REQUEST);
    }
    crate::ws::authorize_socket(&state, &pool, &headers, &id).await?;
    let mut names = Vec::new();
    for member in state.room_hub.online_members(&id) {
        let name: Option<String> = sqlx::query_scalar(
            "SELECT user_name FROM mbr_user WHERE id=? AND user_status='active'",
        )
        .bind(member)
        .fetch_optional(&pool)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        if let Some(name) = name {
            names.push(name);
        }
    }
    names.sort();
    Ok((
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(names),
    ))
}

#[cfg(test)]
mod roster_tests {
    use super::*;
    #[tokio::test]
    async fn roster_requires_room_access_and_exposes_only_online_names() {
        let (state, pool) = super::room_http_tests::fixture().await;
        sqlx::query("INSERT INTO rooms(id,category_room_id,name,desc,created_at,updated_at) VALUES('roster','default-category','Roster','',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO mbr_user(id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at) VALUES('roster-u','roster','hash','roster@example.test','測試會員','active',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        let token = "f".repeat(64);
        sqlx::query("INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at) VALUES('roster-s','roster-u',?,unixepoch()+60)").bind(shared::auth::token_digest(&token)).execute(&pool).await.unwrap();
        assert_eq!(
            room_members(
                State(state.clone()),
                Extension(pool.clone()),
                Path("roster".into()),
                HeaderMap::new()
            )
            .await
            .unwrap_err(),
            StatusCode::UNAUTHORIZED
        );
        let mut headers = HeaderMap::new();
        headers.insert("cookie", format!("chat_session={token}").parse().unwrap());
        let a = state
            .room_hub
            .member_joined("roster".into(), "roster-u".into());
        let b = state
            .room_hub
            .member_joined("roster".into(), "roster-u".into());
        let other = state
            .room_hub
            .member_joined("other-room".into(), "other-u".into());
        let (cache, Json(names)) = room_members(
            State(state.clone()),
            Extension(pool.clone()),
            Path("roster".into()),
            headers.clone(),
        )
        .await
        .unwrap();
        assert_eq!(cache[0].1, "no-store");
        assert_eq!(names, vec!["測試會員"]);
        drop(a);
        drop(b);
        drop(other);
        assert!(
            room_members(
                State(state.clone()),
                Extension(pool.clone()),
                Path("roster".into()),
                headers.clone()
            )
            .await
            .unwrap()
            .1
            .0
            .is_empty()
        );
        sqlx::query("INSERT INTO mbr_exclude(id,mbr_user_id,room_id,created_at,updated_at) VALUES('roster-e','roster-u','roster',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        assert_eq!(
            room_members(
                State(state),
                Extension(pool),
                Path("roster".into()),
                headers
            )
            .await
            .unwrap_err(),
            StatusCode::FORBIDDEN
        );
    }
}
