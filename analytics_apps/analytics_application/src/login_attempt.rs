use shared::login_wait::{WaitState, failed, remaining};

/// 登入等待儲存介面；交易與時鐘由Adapter提供。
pub trait AttemptStore {
    fn read(&mut self) -> impl std::future::Future<Output = Result<WaitState, String>> + Send;
    fn write(
        &mut self,
        state: &WaitState,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;
}

pub async fn pending(repo: &mut impl AttemptStore, now: u64) -> Result<String, String> {
    remaining(&repo.read().await?, now)
}

pub async fn reject(repo: &mut impl AttemptStore, now: u64) -> Result<String, String> {
    let next = failed(&repo.read().await?, now)?;
    let wait = remaining(&next, now)?;
    repo.write(&next).await?;
    Ok(wait)
}

pub async fn accepted(repo: &mut impl AttemptStore) -> Result<(), String> {
    repo.write(&WaitState::default()).await
}
