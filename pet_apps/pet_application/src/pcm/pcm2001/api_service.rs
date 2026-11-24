use std::future::Future;
pub trait PetDeletion {
    fn delete(
        &self,
        id: &str,
        expected_name: &str,
    ) -> impl Future<Output = Result<(), String>> + Send;
}

/// 確認的識別與名稱必須一併送入原子刪除。
pub async fn delete(store: &impl PetDeletion, id: &str, name: &str) -> Result<(), String> {
    if id.is_empty() || name.trim().is_empty() {
        return Err("請確認要刪除的寵物".into());
    }
    store.delete(id, name).await
}
