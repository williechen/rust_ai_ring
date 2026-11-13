use std::time::Duration;

use crate::{
    auth_session::{active_member, session_cookie},
    state::AppState,
};
use axum::{
    Extension,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use chat_domain::message_rules::{room_matches_connection, valid_message_content};
use shared::{ClientEvent, ServerEvent, auth::token_digest};
use sqlx::SqlitePool;
use tokio::{sync::broadcast, time::MissedTickBehavior};

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
    if let Err(status) = authorize_socket(&state, &pool, &headers, &room_id).await {
        return status.into_response();
    }
    ws.on_upgrade(move |socket| handle_socket(socket, state, room_id, pool, headers))
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

#[cfg(test)]
async fn socket_session_active(pool: &SqlitePool, headers: &HeaderMap) -> bool {
    active_member(pool, headers).await.is_ok()
}

async fn authorize_socket(
    state: &AppState,
    pool: &SqlitePool,
    headers: &HeaderMap,
    room: &str,
) -> Result<String, axum::http::StatusCode> {
    use axum::http::StatusCode;
    let member = active_member(pool, headers).await?;
    if crate::member_self::initial(pool, &member).await? {
        return Err(StatusCode::FORBIDDEN);
    }
    if chat_persistence::room_access::is_excluded(pool, &member, room)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let room_matches =
        chat_application::room_presence::exists(state.room_repository.as_ref(), room)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !room_matches {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(member)
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
                if authorize_socket(&state, &pool, &headers, &room_id).await.is_err() {
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
                let member_id = match authorize_socket(&state, &pool, &headers, &room_id).await {
                    Ok(member) => member,
                    Err(_) => { let _ = socket.send(Message::Close(None)).await; break; }
                };
                if let Message::Text(text) = message
                    && let Err(error) = handle_client_event(
                        &mut socket, &state, text.as_str(), room_id.clone(), member_id, &pool, &headers
                    ).await {
                        eprintln!("client message error: {error}");
                        break;
                    }
            }
            event = room_events.recv() => {
                match event {
                    Ok(event) => {
                        if authorize_socket(&state, &pool, &headers, &room_id).await.is_err() {
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

async fn persist_member_event(
    state: &AppState,
    pool: &SqlitePool,
    headers: &HeaderMap,
    room_id: String,
    content: String,
    member_id: String,
) -> Result<bool, sqlx::Error> {
    let Ok(raw) = session_cookie(headers) else {
        return Ok(false);
    };
    let message =
        state
            .chat_service
            .send_member_message(room_id.clone(), content, member_id.clone());
    if !chat_persistence::ws_message::save_authorized(
        pool,
        &token_digest(&raw),
        &member_id,
        &room_id,
        &message.id,
        &message.content,
    )
    .await?
    {
        return Ok(false);
    }
    state
        .room_hub
        .publish(room_id, ServerEvent::MessageCreated(message));
    Ok(true)
}

async fn handle_client_event(
    socket: &mut WebSocket,
    state: &AppState,
    text: &str,
    event_room_id: String,
    member_id: String,
    pool: &SqlitePool,
    headers: &HeaderMap,
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
                        message: "訊息房間與目前連線不符".to_string(),
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

            let (code, message) =
                match persist_member_event(state, pool, headers, room_id, content, member_id).await
                {
                    Ok(true) => return Ok(()),
                    Ok(false) => ("session_expired", "登入或聊天室權限已失效，訊息未送出"),
                    Err(error) => {
                        eprintln!("message persistence failed: {error}");
                        ("message_persistence_failed", "訊息儲存失敗，請稍後重試")
                    }
                };
            send_event(
                socket,
                ServerEvent::Error {
                    code: code.into(),
                    message: message.into(),
                },
            )
            .await?;
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
                            message: "輸入狀態房間與目前連線不符".into(),
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

#[cfg(test)]
mod day47_member_author_tests {
    use super::*;
    use axum::http::{StatusCode, header};
    use chat_application::repository::MessageRepository;
    use chat_domain::MessageAuthor;
    use std::sync::Arc;

    #[sqlx::test(migrations = "../migrations")]
    async fn deleted_room_denies_existing_socket_authorization(pool: SqlitePool) {
        sqlx::query("INSERT INTO mbr_user(id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at) VALUES('member47','member47','hash','member47@example.invalid','會員','active',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        let raw = "a".repeat(64);
        sqlx::query("INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at) VALUES('s47','member47',?1,unixepoch()+60)").bind(shared::auth::token_digest(&raw)).execute(&pool).await.unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("chat_session={raw}").parse().unwrap(),
        );
        let rooms = chat_persistence::SqliteRoomRepository::new(pool.clone());
        chat_application::create_room(
            &rooms,
            chat_application::CreateRoom {
                id: "room47".into(),
                category_room_id: "default-category".into(),
                name: "會員聊天室".into(),
            },
        )
        .await
        .unwrap();
        let state = AppState::new(
            leptos_config::LeptosOptions::builder()
                .output_name("fixture")
                .build(),
            Arc::new(rooms),
            Arc::new(chat_persistence::SqliteMessageRepository::new(pool.clone())),
        );
        assert!(
            authorize_socket(&state, &pool, &headers, "room47")
                .await
                .is_ok()
        );
        sqlx::query("DELETE FROM rooms WHERE id='room47'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            authorize_socket(&state, &pool, &headers, "room47")
                .await
                .unwrap_err(),
            StatusCode::NOT_FOUND
        );
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn session_identity_is_persisted_and_revocation_denies_later_events(pool: SqlitePool) {
        sqlx::query("INSERT INTO mbr_user(id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at) VALUES('member47','member47','hash','member47@example.invalid','會員','active',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        let raw = "a".repeat(64);
        sqlx::query("INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at) VALUES('s47','member47',?1,unixepoch()+60)").bind(shared::auth::token_digest(&raw)).execute(&pool).await.unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("chat_session={raw}").parse().unwrap(),
        );
        let rooms = chat_persistence::SqliteRoomRepository::new(pool.clone());
        chat_application::create_room(
            &rooms,
            chat_application::CreateRoom {
                id: "room47".into(),
                category_room_id: "default-category".into(),
                name: "會員聊天室".into(),
            },
        )
        .await
        .unwrap();
        let messages = chat_persistence::SqliteMessageRepository::new(pool.clone());
        let state = AppState::new(
            leptos_config::LeptosOptions::builder()
                .output_name("fixture")
                .build(),
            Arc::new(rooms),
            Arc::new(chat_persistence::SqliteMessageRepository::new(pool.clone())),
        );
        let (_, mut events) = state.room_hub.subscribe("room47".into());
        let member = authorize_socket(&state, &pool, &headers, "room47")
            .await
            .unwrap();
        assert_eq!(member, "member47");
        assert!(
            persist_member_event(
                &state,
                &pool,
                &headers,
                "room47".into(),
                "會員訊息".into(),
                member
            )
            .await
            .unwrap()
        );
        let history = messages.list_by_room("room47").await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(
            history[0].author,
            MessageAuthor::User {
                user_id: "member47".into()
            }
        );
        match events.try_recv().unwrap() {
            ServerEvent::MessageCreated(message) => assert_eq!(message, history[0]),
            other => panic!("意外事件 {other:?}"),
        }
        sqlx::query("DELETE FROM mbr_session WHERE id='s47'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            authorize_socket(&state, &pool, &headers, "room47")
                .await
                .unwrap_err(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(messages.list_by_room("room47").await.unwrap().len(), 1);
        assert!(events.try_recv().is_err());
    }
}

#[cfg(test)]
mod write_tests;
