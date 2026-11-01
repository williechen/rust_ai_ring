+use chat_application::{CreateRoom, create_room};
use chat_application::repository::{MessageRepository, RoomRepository};
use chat_domain::{ChatMessage, MessageAuthor};
use chat_persistence::{SqliteMessageRepository, SqliteRoomRepository};

#[sqlx::test(migrations = "../migrations")]
async fn saved_history_is_ordered_and_room_scoped(pool: sqlx::SqlitePool) {
    let rooms = SqliteRoomRepository::new(pool.clone());
    let messages = SqliteMessageRepository::new(pool);
    for id in ["room-a", "room-b"] {
        create_room(&rooms, CreateRoom { id: id.into(), category_room_id: "default-category".into(), name: id.into() }).await.unwrap();
    }
    for (id, room) in [("a1", "room-a"), ("b1", "room-b"), ("a2", "room-a")] {
        messages.create(&ChatMessage { id: id.into(), room_id: room.into(), author: MessageAuthor::AnonymousUser, content: id.into() }).await.unwrap();
    }
    let history = messages.list_by_room("room-a").await.unwrap();
    assert_eq!(history.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["a1", "a2"]);
    assert!(messages.list_by_room("missing").await.unwrap().is_empty());
}