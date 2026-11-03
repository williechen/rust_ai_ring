use crate::saved_pet::SavedPet;

/// 僅允許已選且仍存在於最新清單的寵物進入互動操作；不呼叫 IPC。
pub(crate) fn activity_target(pets: &[SavedPet], selected: Option<&str>) -> Option<String> {
    let id = selected?;
    pets.iter()
        .find(|pet| pet.id == id)
        .map(|pet| pet.id.clone())
}
