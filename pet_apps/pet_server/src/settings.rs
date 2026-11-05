use crate::pet_store::{PetStore, SavedPet};
use pet_domain::profile::PetProfile;
use tauri::{Emitter, Manager, State};

#[tauri::command]
pub async fn load_pet_settings(
    id: String,
    store: State<'_, PetStore>,
) -> Result<PetProfile, String> {
    pet_application::settings::load(
        &pet_persistence::settings::SqlitePetSettings(store.pool.clone()),
        &id,
    )
    .await
}

#[tauri::command]
pub async fn save_pet_settings(
    id: String,
    profile: PetProfile,
    store: State<'_, PetStore>,
    app: tauri::AppHandle,
) -> Result<PetProfile, String> {
    let label = crate::pet_windows::label(&id)?;
    let _guard = crate::pet_windows::lifecycle_guard().await;
    let p = pet_application::settings::save(
        &pet_persistence::settings::SqlitePetSettings(store.pool.clone()),
        &id,
        profile,
    )
    .await?;
    if let Some(window) = app.get_webview_window(&label) {
        window
            .set_title(&p.name)
            .map_err(|_| "已儲存設定，但生活窗標題更新失敗")?;
        app.emit_to(
            label,
            "pet-profile-changed",
            SavedPet {
                id,
                name: p.name.clone(),
                species: p.species.clone(),
            },
        )
        .map_err(|_| "已儲存設定，但生活窗同步失敗")?;
    }
    Ok(p)
}
