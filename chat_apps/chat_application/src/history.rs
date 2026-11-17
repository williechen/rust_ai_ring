use crate::repository::{MessageRepository, RepositoryError, RoomRepository};
use chat_domain::ChatMessage;

#[derive(Debug)]
pub enum HistoryError {
    Missing,
    Store(RepositoryError),
}

pub async fn read(
    rooms: &dyn RoomRepository,
    messages: &dyn MessageRepository,
    id: &str,
) -> Result<Vec<ChatMessage>, HistoryError> {
    let room = rooms.find_by_id(id).await.map_err(HistoryError::Store)?;
    if !room.is_some_and(|r| r.id == id) {
        return Err(HistoryError::Missing);
    }
    messages.list_by_room(id).await.map_err(HistoryError::Store)
}
