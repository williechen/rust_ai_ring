use crate::repository::{RepositoryError, RoomRepository};

/// 每次授權都使用既有儲存介面讀取目前房間，不沿用握手快照。
pub async fn exists(rooms: &dyn RoomRepository, room_id: &str) -> Result<bool, RepositoryError> {
    Ok(rooms
        .find_by_id(room_id)
        .await?
        .is_some_and(|room| room.id == room_id))
}
