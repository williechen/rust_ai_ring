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

/// 分類識別及顯示名稱從原房間讀取邊界取得。
pub async fn categories(
    repository: &dyn RoomRepository,
) -> Result<Vec<crate::repository::RoomCategory>, DirectoryError> {
    repository.categories().await.map_err(DirectoryError::Store)
}

/// 名稱與分類代碼共用同一個查詢情境，未知分類不當成全量查詢。
pub async fn filtered(
    repository: &dyn RoomRepository,
    query: &str,
    category: &str,
) -> Result<Vec<Room>, DirectoryError> {
    if query.len() > 200 || query.contains('\0') || category.len() > 200 || category.contains('\0')
    {
        return Err(DirectoryError::InvalidQuery);
    }
    if !category.is_empty()
        && !categories(repository)
            .await?
            .iter()
            .any(|c| c.code == category)
    {
        return Err(DirectoryError::InvalidQuery);
    }
    repository
        .directory(query, category)
        .await
        .map_err(DirectoryError::Store)
}

/// 只暴露每個房間去重後的人數，不洩漏會員身分或 Session。
pub trait OnlineRoomCounts: Send + Sync {
    fn snapshot(&self) -> std::collections::HashMap<String, usize>;
}

pub struct PopularRoom {
    pub room: Room,
    pub online_count: usize,
}

pub fn rank_popular(
    rooms: Vec<Room>,
    counts: &std::collections::HashMap<String, usize>,
) -> Vec<PopularRoom> {
    let mut ranked: Vec<_> = rooms
        .into_iter()
        .map(|room| PopularRoom {
            online_count: counts.get(&room.id).copied().unwrap_or(0),
            room,
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.online_count
            .cmp(&a.online_count)
            .then_with(|| a.room.name.cmp(&b.room.name))
            .then_with(|| a.room.id.cmp(&b.room.id))
    });
    ranked.truncate(100);
    ranked
}

pub async fn popular(
    repository: &dyn RoomRepository,
    presence: &dyn OnlineRoomCounts,
) -> Result<Vec<PopularRoom>, DirectoryError> {
    let rooms = repository.list().await.map_err(DirectoryError::Store)?;
    Ok(rank_popular(rooms, &presence.snapshot()))
}

#[cfg(test)]
mod popular_tests {
    use super::*;
    #[test]
    fn ranking_uses_real_counts_then_name_and_id_and_limits_one_hundred() {
        let rooms = (0..112)
            .map(|i| Room {
                id: format!("r{i:02}"),
                name: format!("房{i:02}"),
                category_room_id: "default-category".into(),
                category_label: None,
            })
            .collect();
        let counts = std::collections::HashMap::from([
            ("r11".into(), 3),
            ("r10".into(), 2),
            ("deleted".into(), 99),
        ]);
        let result = rank_popular(rooms, &counts);
        assert_eq!(result.len(), 100);
        assert_eq!(result[0].room.id, "r11");
        assert_eq!(result[0].online_count, 3);
        assert_eq!(result[1].room.id, "r10");
        assert_eq!(result[2].room.id, "r00");
        assert_eq!(result[2].online_count, 0);
    }
}
