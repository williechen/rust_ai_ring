use std::future::Future;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedPet {
    pub id: String,
    pub name: String,
    pub species: String,
}

pub trait PetNames {
    fn defaults(&self) -> impl Future<Output = Result<Vec<NamedPet>, String>> + Send {
        async { Err("啟動預設讀取未接通".into()) }
    }
    fn create(&self, name: &str) -> impl Future<Output = Result<NamedPet, String>> + Send {
        let _ = name;
        async { Err("寵物建立未接通".into()) }
    }
    fn active(&self) -> impl Future<Output = Result<Option<NamedPet>, String>> + Send {
        async { Err("生活窗選取讀取未接通".into()) }
    }
    fn list(&self) -> impl Future<Output = Result<Vec<NamedPet>, String>> + Send {
        async { Err("寵物清單未接通".into()) }
    }
    fn find(&self, id: &str) -> impl Future<Output = Result<NamedPet, String>> + Send;
    fn rename(&self, id: &str, name: &str)
    -> impl Future<Output = Result<NamedPet, String>> + Send;
}

/// 名稱規則先於持久化；Port 不持有 SQLx 或 Tauri。
pub async fn rename_pet(store: &impl PetNames, id: &str, input: &str) -> Result<NamedPet, String> {
    let name = crate::normalize_new_pet_name(input)?;
    store.rename(id, &name).await
}

pub async fn read_pet(store: &impl PetNames, id: &str) -> Result<NamedPet, String> {
    store.find(id).await
}

pub async fn list_pets(store: &impl PetNames) -> Result<Vec<NamedPet>, String> {
    store.list().await
}

pub async fn active_pet(store: &impl PetNames) -> Result<Option<NamedPet>, String> {
    store.active().await
}

pub async fn create_pet(store: &impl PetNames, input: &str) -> Result<NamedPet, String> {
    let name = crate::normalize_new_pet_name(input)?;
    store.create(&name).await
}

pub async fn default_pets(store: &impl PetNames) -> Result<Vec<NamedPet>, String> {
    store.defaults().await
}
