use async_trait::async_trait;
use chat_domain::{ChatMessage, Room};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("房間存取錯誤: {0}")]
    RoomAccessError(String),
    #[error("訊息存取錯誤: {0}")]
    MessageAccessError(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomCategory {
    pub code: String,
    pub label: String,
}

#[async_trait]
pub trait RoomRepository: Send + Sync {
    async fn categories(&self) -> Result<Vec<RoomCategory>, RepositoryError> {
        Err(RepositoryError::RoomAccessError("分類目錄未接通".into()))
    }
    async fn directory(&self, query: &str, category: &str) -> Result<Vec<Room>, RepositoryError> {
        if !category.is_empty() {
            return Err(RepositoryError::RoomAccessError("分類查詢未接通".into()));
        }
        if query.is_empty() {
            self.list().await
        } else {
            self.search_by_name(query).await
        }
    }
    async fn find_by_id(&self, room_id: &str) -> Result<Option<Room>, RepositoryError>;

    async fn create(&self, room: &Room) -> Result<(), RepositoryError>;

    async fn list(&self) -> Result<Vec<Room>, RepositoryError>;

    async fn search_by_name(&self, query: &str) -> Result<Vec<Room>, RepositoryError>;
}

#[async_trait]
pub trait MessageRepository: Send + Sync {
    async fn list_by_room(&self, room_id: &str) -> Result<Vec<ChatMessage>, RepositoryError>;
    async fn create(&self, message: &ChatMessage) -> Result<(), RepositoryError>;
}
