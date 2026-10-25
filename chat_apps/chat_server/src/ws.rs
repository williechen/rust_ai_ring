use std::time::Duration;

use crate::{auth_session::active_member, state::AppState};
use axum::{
    Extension,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::HeaderMap,
    response::Response,
};
use shared::{ClientEvent, ServerEvent};
use sqlx::SqlitePool;
use tokio::{sync::broadcast, time::MissedTickBehavior};

use crate::hub::RoomHub;
use chat_application::{
    ChatService,
    repository::{MessageRepository, RepositoryError, RoomRepository},
};

async fn send_event(
    socket: &mut WebSocket,
    event: ServerEvent,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let json = serde_json::to_string(&event)?;
    socket.send(Message::Text(json.into())).await?;
    Ok(())
}

pub async fn ws_handler(
    Path(room_id): Path<String>,
    State(state): State<AppState>,
    Extension(pool): Extension<SqlitePool>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    match state.room_repository.find_by_id(&room_id).await {
        Ok(Some(_)) => {
            ws.on_upgrade(move |socket| handle_socket(socket, state, room_id, pool, headers))
        }
        Ok(None) => Response::builder()
            .status(404)
            .body("Room not found".into())
            .unwrap(),
        Err(error) => {
            eprintln!("websocket room lookup error: {error:?}");
            Response::builder()
                .status(500)
                .body("Internal server error".into())
                .unwrap()
        }
    }
}

// 訊息內容不可僅有空白；保留原始內容，不自動裁切。
fn valid_message_content(content: &str) -> bool {
    !content.trim().is_empty()
}

// 防止跨房間發送；空白識別碼不可視為合法房間。
fn room_matches_connection(connection: &str, event: &str) -> bool {
    !connection.trim().is_empty() && !event.trim().is_empty() && connection == event
}

fn typing_event_for_room(
    connection_room: &str,
    event_room: &str,
    active: bool,
) -> Option<ServerEvent> {
    room_matches_connection(connection_room, event_room).then(|| ServerEvent::TypingChanged {
        room_id: connection_room.to_owned(),
        active,
    })
}

async fn socket_session_active(pool: &SqlitePool, headers: &HeaderMap) -> bool {
    active_member(pool, headers).await.is_ok()
}

async fn handle_socket(
    mut socket: WebSocket,
    state: AppState,
    room_id: String,
    pool: SqlitePool,
    headers: HeaderMap,
) {
    let (room_sender, mut room_events) = state.room_hub.subscribe(room_id.clone());
    let mut recheck = tokio::time::interval(Duration::from_secs(1));
    recheck.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = recheck.tick() => {
                if !socket_session_active(&pool, &headers).await {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
            }
            incoming = socket.recv() => {
                let Some(result) = incoming else { break; };
                let message = match result {
                    Ok(message) => message,
                    Err(error) => {
                        eprintln!("websocket receive error: {error}");
                        break;
                    }
                };
                // 不因握手時曾通過就永久信任 Cookie。
                if !socket_session_active(&pool, &headers).await {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
                if let Message::Text(text) = message
                    && let Err(error) = handle_client_event(
                        &mut socket, &state, text.as_str(), room_id.clone()
                    ).await {
                        eprintln!("client message error: {error}");
                        break;
                    }
            }
            event = room_events.recv() => {
                match event {
                    Ok(event) => {
                        if !socket_session_active(&pool, &headers).await {
                            let _ = socket.send(Message::Close(None)).await;
                            break;
                        }
                        if let Err(error) = send_event(&mut socket, event).await {
                            eprintln!("websocket send error: {error}");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        eprintln!("websocket receiver lagged skipped: {skipped} events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    drop(room_events);
    state.room_hub.cleanup(room_id, &room_sender);
}

async fn persist_ws_message(
    service: &ChatService,
    rooms: &dyn RoomRepository,
    messages: &dyn MessageRepository,
    hub: &RoomHub,
    room_id: String,
    content: String,
) -> Result<(), RepositoryError> {
    let message = service.send_message(room_id.clone(), content);
    chat_application::persist_in_existing_room(rooms, messages, message, move |saved| {
        hub.publish(room_id, ServerEvent::MessageCreated(saved))
    })
    .await
}

async fn handle_client_event(
    socket: &mut WebSocket,
    state: &AppState,
    text: &str,
    event_room_id: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let event: ClientEvent = serde_json::from_str(text)?;

    match event {
        ClientEvent::Ping => {
            send_event(socket, ServerEvent::Pong).await?;
        }
        ClientEvent::SendMessage { room_id, content } => {
            if !room_matches_connection(&event_room_id, &room_id) {
                send_event(
                    socket,
                    ServerEvent::Error {
                        code: "room_mismatch".to_string(),
                        message: "message room does not match connection room".to_string(),
                    },
                )
                .await?;
                return Ok(());
            }

            if !valid_message_content(&content) {
                send_event(
                    socket,
                    ServerEvent::Error {
                        code: "invalid_message_content".to_owned(),
                        message: "訊息不可為空白".to_owned(),
                    },
                )
                .await?;
                return Ok(());
            }

            if let Err(error) = persist_ws_message(
                state.chat_service.as_ref(),
                state.room_repository.as_ref(),
                state.message_repository.as_ref(),
                &state.room_hub,
                room_id,
                content,
            )
            .await
            {
                eprintln!("message persistence failed: {error}");
                send_event(
                    socket,
                    ServerEvent::Error {
                        code: "message_persistence_failed".to_owned(),
                        message: "訊息儲存失敗，請稍後重試".to_owned(),
                    },
                )
                .await?;
                return Ok(());
            }
        }
        ClientEvent::Typing { room_id, active } => {
            match typing_event_for_room(&event_room_id, &room_id, active) {
                Some(event) => {
                    // 共用既有 RoomHub，不產生 SQLite 訊息紀錄。
                    state.room_hub.publish(event_room_id, event);
                }
                None => {
                    send_event(
                        socket,
                        ServerEvent::Error {
                            code: "room_mismatch".into(),
                            message: "typing room does not match connection room".into(),
                        },
                    )
                    .await?;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::{room_matches_connection, valid_message_content};

    #[test]
    fn matching_room() {
        assert!(room_matches_connection("room-a", "room-a"));
    }
    #[test]
    fn different_room() {
        assert!(!room_matches_connection("room-a", "room-b"));
    }
    #[test]
    fn blank_room() {
        assert!(!room_matches_connection("", ""));
        assert!(!room_matches_connection("room-a", " "));
    }
    #[test]
    fn accepts_nonblank_content() {
        assert!(valid_message_content("你好"));
        assert!(valid_message_content(" 你好 "));
    }
    #[test]
    fn rejects_blank_content() {
        assert!(!valid_message_content(""));
        assert!(!valid_message_content(" \n\t "));
    }

    // 加到 apps/server/src/ws.rs 既有 #[cfg(test)] mod test 內。
    #[sqlx::test(migrations = "../migrations")]
    async fn ws_message_requires_room_at_write_time(pool: sqlx::SqlitePool) {
        use super::persist_ws_message;
        use crate::hub::RoomHub;
        use chat_application::repository::RepositoryError;
        use chat_application::{ChatService, CreateRoom, create_room};
        use shared::ServerEvent;

        let rooms = chat_persistence::SqliteRoomRepository::new(pool.clone());
        let messages = chat_persistence::SqliteMessageRepository::new(pool.clone());
        let hub = RoomHub::new(8);
        let (_, mut events) = hub.subscribe("room-day11".into());
        let service = ChatService::new();

        // 握手後若房間已不存在，寫入時仍須拒絕。
        let denied = persist_ws_message(
            &service,
            &rooms,
            &messages,
            &hub,
            "room-day11".into(),
            "不應寫入".into(),
        )
        .await;
        assert!(matches!(denied, Err(RepositoryError::RoomAccessError(_))));
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0);
        assert!(events.try_recv().is_err());

        create_room(
            &rooms,
            CreateRoom {
                id: "room-day11".into(),
                category_room_id: "default-category".into(),
                name: "測試房間".into(),
            },
        )
        .await
        .unwrap();

        persist_ws_message(
            &service,
            &rooms,
            &messages,
            &hub,
            "room-day11".into(),
            "確實寫入".into(),
        )
        .await
        .unwrap();

        match events.try_recv().expect("應通知訂閱者一次") {
            ServerEvent::MessageCreated(message) => {
                assert_eq!(message.room_id, "room-day11");
                assert_eq!(message.content, "確實寫入");
            }
            other => panic!("意外事件: {other:?}"),
        }
        assert!(events.try_recv().is_err());
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 1);
    }

    #[test]
    fn typing_is_only_forwarded_to_connection_room() {
        use super::typing_event_for_room;
        assert!(typing_event_for_room("room-a", "room-b", true).is_none());
        assert!(typing_event_for_room("room-a", " ", true).is_none());
        assert_eq!(
            typing_event_for_room("room-a", "room-a", false),
            Some(shared::ServerEvent::TypingChanged {
                room_id: "room-a".into(),
                active: false,
            })
        );
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn day36_revoked_session_is_not_active(pool: sqlx::SqlitePool) {
        use super::socket_session_active;
        use axum::http::{HeaderMap, header};
        use shared::auth::token_digest;
        let raw = "a".repeat(64);
        sqlx::query(
        "INSERT INTO mbr_user
         (id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at)
         VALUES('day36-user','day36','hash','day36@example.invalid','Day36','active',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)"
    ).execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at)
         VALUES('day36-session','day36-user',?1,unixepoch()+60)",
        )
        .bind(token_digest(&raw))
        .execute(&pool)
        .await
        .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("chat_session={raw}").parse().unwrap(),
        );
        assert!(socket_session_active(&pool, &headers).await);
        sqlx::query("DELETE FROM mbr_session WHERE id='day36-session'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(!socket_session_active(&pool, &headers).await);
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn day36_database_error_fails_closed(pool: sqlx::SqlitePool) {
        use super::socket_session_active;
        use axum::http::{HeaderMap, header};
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("chat_session={}", "a".repeat(64)).parse().unwrap(),
        );
        pool.close().await;
        assert!(!socket_session_active(&pool, &headers).await);
    }
}
