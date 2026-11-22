use crate::repository::{MessageRepository, RepositoryError, RoomRepository};
use chat_domain::ChatMessage;

#[derive(Debug)]
pub enum HistoryError {
    Invalid,
    Missing,
    Store(RepositoryError),
}

pub async fn read(
    rooms: &dyn RoomRepository,
    messages: &dyn MessageRepository,
    id: &str,
) -> Result<Vec<ChatMessage>, HistoryError> {
    validate_id(id)?;
    let room = rooms.find_by_id(id).await.map_err(HistoryError::Store)?;
    if !room.is_some_and(|r| r.id == id) {
        return Err(HistoryError::Missing);
    }
    messages.list_by_room(id).await.map_err(HistoryError::Store)
}

/// 歷史 HTTP 與非 HTTP caller 共用相同識別規則。
pub fn validate_id(id: &str) -> Result<(), HistoryError> {
    if id.trim().is_empty() || id.len() > 40 || id.contains('\0') {
        Err(HistoryError::Invalid)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod validation_tests {
    #[test]
    fn historical_ids_stay_supported() {
        for id in ["r", "historical-room"] {
            assert!(super::validate_id(id).is_ok());
        }
        for id in ["", " ", "a\0b", &"a".repeat(41)] {
            assert!(super::validate_id(id).is_err());
        }
    }
}
