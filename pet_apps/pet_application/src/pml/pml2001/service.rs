use std::future::Future;
/// 籠內狀態讀取由應用層契約隔離儲存細節。
pub trait CageQuery {
    fn read(&self, id: &str) -> impl Future<Output = Result<bool, String>> + Send;
}
pub async fn read(store: &impl CageQuery, id: &str) -> Result<bool, String> {
    store.read(id).await
}
