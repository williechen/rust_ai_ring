pub mod product_event;
pub mod repository;

use chat_domain::{ChatMessage, MessageAuthor, Room};
use uuid::Uuid;

use crate::repository::{MessageRepository, RepositoryError, RoomRepository};

#[derive(Debug, Clone, Default)]
pub struct ChatService;

impl ChatService {
    pub fn new() -> Self {
        Self
    }

    pub fn send_message(&self, room_id: String, content: String) -> ChatMessage {
        ChatMessage {
            id: Uuid::new_v4().to_string(),
            room_id,
            author: MessageAuthor::AnonymousUser,
            content,
        }
    }
}

/// 表單與 JSON 建房共用；保留合法輸入原字串。
pub fn validate_room_name(name: &str) -> Result<(), &'static str> {
    if name.trim().is_empty() {
        return Err("聊天室名稱不得為空白");
    }
    if name.len() > 200 {
        return Err("聊天室名稱最多 200 UTF-8 bytes");
    }
    if name.chars().any(char::is_control) {
        return Err("聊天室名稱不得含控制字元");
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct CreateRoom {
    pub id: String,
    pub category_room_id: String,
    pub name: String,
}

pub async fn create_room(
    room_repository: &dyn RoomRepository,
    input: CreateRoom,
) -> Result<Room, RepositoryError> {
    let room = Room {
        id: input.id,
        category_room_id: input.category_room_id,
        name: input.name,
    };
    room_repository.create(&room).await?;
    Ok(room)
}

///成功 create 後才推播；DB 失敗時 closure 不執行。
pub async fn persist_then_publish(
    repository: &dyn MessageRepository,
    message: ChatMessage,
    publish: impl FnOnce(ChatMessage),
) -> Result<(), RepositoryError> {
    repository.create(&message).await?;
    publish(message);
    Ok(())
}

/// 確認查到的房間 ID 與訊息目標一致，才進行寫入及通知。
pub async fn persist_in_existing_room(
    rooms: &dyn RoomRepository,
    messages: &dyn MessageRepository,
    message: ChatMessage,
    publish: impl FnOnce(ChatMessage),
) -> Result<(), RepositoryError> {
    match rooms.find_by_id(&message.room_id).await? {
        Some(room) if room.id == message.room_id => {
            persist_then_publish(messages, message, publish).await
        }
        Some(_) => Err(RepositoryError::RoomAccessError("房間 ID 不一致".into())),
        None => Err(RepositoryError::RoomAccessError("房間不存在".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct FailingRepository;
    #[async_trait]
    impl MessageRepository for FailingRepository {
        async fn create(&self, _: &ChatMessage) -> Result<(), RepositoryError> {
            Err(RepositoryError::MessageAccessError("DB unavailable".into()))
        }
    }

    struct SuccessfulRepository;
    #[async_trait]
    impl MessageRepository for SuccessfulRepository {
        async fn create(&self, _: &ChatMessage) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    struct ExistingRoom;
    #[async_trait]
    impl RoomRepository for ExistingRoom {
        async fn find_by_id(&self, id: &str) -> Result<Option<Room>, RepositoryError> {
            Ok(Some(Room {
                id: id.into(),
                category_room_id: "c1".into(),
                name: "room".into(),
            }))
        }
        async fn create(&self, _: &Room) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn list(&self) -> Result<Vec<Room>, RepositoryError> {
            Ok(Vec::new())
        }
        async fn search_by_name(&self, _: &str) -> Result<Vec<Room>, RepositoryError> {
            Ok(Vec::new())
        }
    }

    struct MissingRoom;
    #[async_trait]
    impl RoomRepository for MissingRoom {
        async fn find_by_id(&self, _: &str) -> Result<Option<Room>, RepositoryError> {
            Ok(None)
        }
        async fn create(&self, _: &Room) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn list(&self) -> Result<Vec<Room>, RepositoryError> {
            Ok(Vec::new())
        }
        async fn search_by_name(&self, _: &str) -> Result<Vec<Room>, RepositoryError> {
            Ok(Vec::new())
        }
    }

    struct WrongRoom;
    #[async_trait]
    impl RoomRepository for WrongRoom {
        async fn find_by_id(&self, _: &str) -> Result<Option<Room>, RepositoryError> {
            Ok(Some(Room {
                id: "another-room".into(),
                category_room_id: "category".into(),
                name: "其他房間".into(),
            }))
        }
        async fn create(&self, _: &Room) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn list(&self) -> Result<Vec<Room>, RepositoryError> {
            Ok(Vec::new())
        }
        async fn search_by_name(&self, _: &str) -> Result<Vec<Room>, RepositoryError> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn failed_write_never_publishes() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let message = ChatMessage {
            id: "m-1".into(),
            room_id: "r-1".into(),
            author: MessageAuthor::AnonymousUser,
            content: "hello".into(),
        };
        let result = persist_then_publish(&FailingRepository, message, move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
        })
        .await;
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn successful_write_publishes_exactly_once() {
        let publishes = Arc::new(AtomicUsize::new(0));
        let observed = publishes.clone();
        let message = ChatMessage {
            id: "m-success".into(),
            room_id: "r-success".into(),
            author: MessageAuthor::AnonymousUser,
            content: "hello".into(),
        };

        persist_then_publish(&SuccessfulRepository, message, move |published| {
            assert_eq!(published.id, "m-success");
            observed.fetch_add(1, Ordering::SeqCst);
        })
        .await
        .expect("successful storage must allow publish");

        assert_eq!(publishes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn missing_room_blocks_message_and_publish() {
        let mut calls = 0;
        let msg = ChatMessage {
            id: "m".into(),
            room_id: "absent".into(),
            author: MessageAuthor::AnonymousUser,
            content: "hi".into(),
        };
        let result =
            persist_in_existing_room(&MissingRoom, &SuccessfulRepository, msg, |_| calls += 1)
                .await;
        assert!(matches!(result, Err(RepositoryError::RoomAccessError(_))));
        assert_eq!(calls, 0);
    }

    #[tokio::test]
    async fn existing_room_allows_persist_and_publish() {
        let mut calls = 0;
        let msg = ChatMessage {
            id: "m".into(),
            room_id: "r1".into(),
            author: MessageAuthor::AnonymousUser,
            content: "hi".into(),
        };
        persist_in_existing_room(&ExistingRoom, &SuccessfulRepository, msg, |_| calls += 1)
            .await
            .unwrap();
        assert_eq!(calls, 1);
    }

    #[tokio::test]
    async fn mismatched_room_id_must_not_persist_or_publish() {
        let mut published = 0;
        let message = ChatMessage {
            id: "message-1".into(),
            room_id: "requested-room".into(),
            author: MessageAuthor::AnonymousUser,
            content: "hello".into(),
        };
        let result = persist_in_existing_room(&WrongRoom, &SuccessfulRepository, message, |_| {
            published += 1
        })
        .await;
        assert!(matches!(result, Err(RepositoryError::RoomAccessError(_))));
        assert_eq!(published, 0);
    }
}

#[cfg(test)]
mod day35_room_name_tests {
    use super::validate_room_name;

    #[test]
    fn room_name_policy_is_utf8_bytes_and_control_safe() {
        for ok in ["中文房間", " 房間 ", &"a".repeat(200)] {
            assert_eq!(validate_room_name(ok), Ok(()));
        }
        for bad in [
            "",
            "  ",
            "房\n間",
            "房\t間",
            "房\r間",
            "房\0間",
            "\u{7f}",
            &"房".repeat(67),
        ] {
            assert!(validate_room_name(bad).is_err(), "{bad:?}");
        }
    }
}
