use crate::pet_store::{PetStore, SavedPet};
use pet_application::settings::{ProfileNotification, SettingsSaved};
use pet_domain::profile::PetProfile;
use tauri::{Emitter, Manager, State};

struct WindowNotification<'a>(&'a tauri::AppHandle);
impl ProfileNotification for WindowNotification<'_> {
    fn notify(&self, id: &str, p: &PetProfile) -> Result<(), String> {
        let label = crate::pet_windows::label(id)?;
        if let Some(window) = self.0.get_webview_window(&label) {
            window
                .set_title(&p.name)
                .map_err(|_| "生活窗標題更新失敗")?;
            self.0
                .emit_to(
                    label,
                    "pet-profile-changed",
                    SavedPet {
                        id: id.into(),
                        name: p.name.clone(),
                        species: p.species.clone(),
                    },
                )
                .map_err(|_| "生活窗設定通知失敗")?;
        }
        Ok(())
    }
}

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
) -> Result<SettingsSaved, String> {
    crate::pet_windows::label(&id)?;
    let _guard = crate::pet_windows::lifecycle_guard().await;
    pet_application::settings::save_and_notify(
        &pet_persistence::settings::SqlitePetSettings(store.pool.clone()),
        &WindowNotification(&app),
        &id,
        profile,
    )
    .await
}

#[tauri::command]
pub async fn resync_pet_settings(
    id: String,
    store: State<'_, PetStore>,
    app: tauri::AppHandle,
) -> Result<SettingsSaved, String> {
    crate::pet_windows::label(&id)?;
    let _guard = crate::pet_windows::lifecycle_guard().await;
    pet_application::settings::resynchronize(
        &pet_persistence::settings::SqlitePetSettings(store.pool.clone()),
        &WindowNotification(&app),
        &id,
    )
    .await
}

#[tauri::command]
pub async fn save_pet_settings_checked(
    id: String,
    profile: PetProfile,
    expected: PetProfile,
    store: State<'_, PetStore>,
    app: tauri::AppHandle,
) -> Result<SettingsSaved, String> {
    crate::pet_windows::label(&id)?;
    let _guard = crate::pet_windows::lifecycle_guard().await;
    pet_application::settings::save_checked_and_notify(
        &pet_persistence::settings::SqlitePetSettings(store.pool.clone()),
        &WindowNotification(&app),
        &id,
        profile,
        expected,
    )
    .await
}
