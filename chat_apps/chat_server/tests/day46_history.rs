use axum::{
    Extension,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
};
use chat_application::repository::MessageRepository;
use chat_domain::{ChatMessage, MessageAuthor};
use chat_server::{http::room_history, state::AppState};
use std::sync::Arc;

#[tokio::test]
async fn history_denies_anonymous_and_excluded_member() {
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    chat_persistence::migrate(&db).await.unwrap();
    sqlx::query("INSERT INTO rooms(id,category_room_id,name,desc,created_at,updated_at) VALUES('room-a','default-category','房間','',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").execute(&db).await.unwrap();
    sqlx::query("INSERT INTO mbr_user(id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at) VALUES('u46','u46','hash','u46@example.invalid','User','active',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").execute(&db).await.unwrap();
    let raw = "e".repeat(64);
    sqlx::query("INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at) VALUES('s46','u46',?1,unixepoch()+60)").bind(shared::auth::token_digest(&raw)).execute(&db).await.unwrap();
    let repo = Arc::new(chat_persistence::SqliteMessageRepository::new(db.clone()));
    repo.create(&ChatMessage {
        id: "saved".into(),
        room_id: "room-a".into(),
        author: MessageAuthor::AnonymousUser,
        content: "已儲存".into(),
    })
    .await
    .unwrap();
    let state = AppState::new(
        leptos_config::get_configuration(None)
            .unwrap()
            .leptos_options,
        Arc::new(chat_persistence::SqliteRoomRepository::new(db.clone())),
        repo,
    );
    assert_eq!(
        room_history(
            State(state.clone()),
            Extension(db.clone()),
            Path("room-a".into()),
            HeaderMap::new()
        )
        .await
        .unwrap_err(),
        StatusCode::UNAUTHORIZED
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        header::COOKIE,
        format!("chat_session={raw}").parse().unwrap(),
    );
    let result = room_history(
        State(state.clone()),
        Extension(db.clone()),
        Path("room-a".into()),
        headers.clone(),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].content, "已儲存");
    sqlx::query("INSERT INTO mbr_exclude(id,mbr_user_id,room_id) VALUES('e46','u46','room-a')")
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        room_history(
            State(state),
            Extension(db.clone()),
            Path("room-a".into()),
            headers
        )
        .await
        .unwrap_err(),
        StatusCode::FORBIDDEN
    );
    db.close().await;
}
