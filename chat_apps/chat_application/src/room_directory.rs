use crate::repository::{RepositoryError, RoomRepository};
use chat_domain::Room;

#[derive(Debug)]
pub enum DirectoryError {
    InvalidQuery,
    Store(RepositoryError),
}

/// 公開目錄的輸入界線與 Repository 選擇集中於 Application。
pub async fn list(
    repository: &dyn RoomRepository,
    query: &str,
) -> Result<Vec<Room>, DirectoryError> {
    if query.len() > 200 || query.contains('\0') {
        return Err(DirectoryError::InvalidQuery);
    }
    let result = if query.is_empty() {
        repository.list().await
    } else {
        repository.search_by_name(query).await
    };
    result.map_err(DirectoryError::Store)
}
