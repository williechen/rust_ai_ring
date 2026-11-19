use crate::repository::{RepositoryError, RoomRepository};
use chat_domain::Room;
#[derive(Debug)]
pub enum DetailsError {
    Missing,
    Mismatch,
    Store(RepositoryError),
}
pub async fn read(store: &dyn RoomRepository, id: &str) -> Result<Room, DetailsError> {
    let room = store
        .find_by_id(id)
        .await
        .map_err(DetailsError::Store)?
        .ok_or(DetailsError::Missing)?;
    if room.id != id {
        return Err(DetailsError::Mismatch);
    }
    Ok(room)
}
