use crate::state::AppState;
use axum::extract::{Query, State};
use axum::{Json, http::StatusCode};
use serde::Serialize;
use shared::{CreateRoomRequest, CreateRoomResponse};

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
    pub name: String,
}

#[derive(Default, serde::Deserialize)]
pub struct RoomSearch {
    #[serde(default)]
    q: String,
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
        name: room.name,
    }))
}

pub async fn list_rooms(
    State(state): State<AppState>,
    Query(search): Query<RoomSearch>,
) -> Result<Json<Vec<RoomDetails>>, StatusCode> {
    if search.q.len() > 200 || search.q.contains('\0') {
        return Err(StatusCode::BAD_REQUEST);
    }
    let rooms = if search.q.is_empty() {
        state
            .room_repository
            .list()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        state
            .room_repository
            .search_by_name(&search.q)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    Ok(Json(
        rooms
            .into_iter()
            .map(|room| RoomDetails {
                id: room.id,
                category_room_id: room.category_room_id,
                name: room.name,
            })
            .collect(),
    ))
}

#[cfg(test)]
mod room_http_tests {
    use super::*;
    use axum::response::IntoResponse;
    use std::sync::Arc;

    async fn fixture() -> (AppState, sqlx::SqlitePool) {
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

    #[tokio::test]
    async fn list_rooms_returns_persisted_room() {
        let (state, pool) = fixture().await;
        let response = list_rooms(State(state), Query(RoomSearch { q: "".to_string() }))
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
}
