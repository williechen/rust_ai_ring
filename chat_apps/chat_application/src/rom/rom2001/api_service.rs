pub use super::api_dto::MemberRoomInput;
use std::future::Future;

pub trait MemberRoomStore {
    type Error;
    fn authorize(&self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Adapter須在同一寫入交易重驗Session／權限及分類，再提交房間。
    fn create_checked(
        &self,
        input: MemberRoomInput,
    ) -> impl Future<Output = Result<String, Self::Error>> + Send;
}

#[derive(Debug)]
pub enum MemberRoomError<E> {
    InvalidName,
    Store(E),
}

pub async fn create<S: MemberRoomStore>(
    store: &S,
    input: MemberRoomInput,
) -> Result<String, MemberRoomError<S::Error>> {
    store.authorize().await.map_err(MemberRoomError::Store)?;
    crate::validate_room_name(&input.name).map_err(|_| MemberRoomError::InvalidName)?;
    store
        .create_checked(input)
        .await
        .map_err(MemberRoomError::Store)
}
