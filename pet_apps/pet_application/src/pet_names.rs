use std::future::Future;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedPet {
    pub id: String,
    pub name: String,
    pub species: String,
}

pub trait PetNames {
    fn rename(&self, id: &str, name: &str)
    -> impl Future<Output = Result<NamedPet, String>> + Send;
}

/// 名稱規則先於持久化；Port 不持有 SQLx 或 Tauri。
pub async fn rename_pet(store: &impl PetNames, id: &str, input: &str) -> Result<NamedPet, String> {
    let name = crate::normalize_new_pet_name(input)?;
    store.rename(id, &name).await
}
