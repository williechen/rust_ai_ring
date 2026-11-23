pub use pet_domain::profile::PetProfile as SettingsProfile;
pub trait PetSettings {
    fn save_checked(
        &self,
        id: &str,
        p: &SettingsProfile,
        expected: &SettingsProfile,
    ) -> impl std::future::Future<Output = Result<SettingsProfile, String>> + Send {
        let _ = (id, p, expected);
        async { Err("設定快照比對未接通".into()) }
    }
    fn load(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<SettingsProfile, String>> + Send;
    fn save(
        &self,
        id: &str,
        profile: &SettingsProfile,
    ) -> impl std::future::Future<Output = Result<SettingsProfile, String>> + Send;
}

/// 五欄設定只接受目前有正式素材的烏龜，不把其他物種冒充已實作。
pub fn normalize(mut p: SettingsProfile) -> Result<SettingsProfile, String> {
    p.name = crate::normalize_new_pet_name(&p.name)?;
    p.species = p.species.trim().to_owned();
    if p.species != "tortoise" {
        return Err("目前支援種類為 tortoise（烏龜）".into());
    }
    if p.appearance.is_some() {
        return Err("外觀覆寫尚未接通".into());
    }
    for tags in [&mut p.likes, &mut p.personality, &mut p.interests] {
        if tags.len() > 32 {
            return Err("每欄最多32個項目".into());
        }
        for value in tags.iter_mut() {
            *value = value.trim().to_owned();
            if value.is_empty()
                || value.chars().count() > 255
                || value.chars().any(char::is_control)
            {
                return Err("設定項目須為1到255字且不含控制字元".into());
            }
        }
        tags.sort();
        tags.dedup();
    }
    p.validate().map_err(|e| e.to_string())?;
    Ok(p)
}

pub async fn load(store: &impl PetSettings, id: &str) -> Result<SettingsProfile, String> {
    store.load(id).await
}

pub async fn save(
    store: &impl PetSettings,
    id: &str,
    p: SettingsProfile,
) -> Result<SettingsProfile, String> {
    let p = normalize(p)?;
    store.save(id, &p).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_preserve_name_rules_and_normalize_tag_sets() {
        let p = SettingsProfile {
            name: " 小龜 ".into(),
            species: "tortoise".into(),
            likes: vec![" 太陽 ".into(), "太陽".into()],
            personality: vec![],
            interests: vec![],
            appearance: None,
        };
        let p = normalize(p).unwrap();
        assert_eq!(p.name, "小龜");
        assert_eq!(p.likes, vec!["太陽"]);
        assert!(
            normalize(SettingsProfile {
                name: "".into(),
                ..p.clone()
            })
            .is_err()
        );
        assert!(
            normalize(SettingsProfile {
                species: "cat".into(),
                ..p
            })
            .is_err()
        );
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SettingsSaved {
    #[serde(flatten)]
    pub profile: SettingsProfile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_warning: Option<String>,
}
pub trait ProfileNotification {
    fn notify(&self, id: &str, profile: &SettingsProfile) -> Result<(), String>;
}
/// 持久化成功後，通知失敗只能回報待同步，不得偽裝儲存失敗。
pub async fn save_and_notify(
    store: &impl PetSettings,
    notification: &impl ProfileNotification,
    id: &str,
    profile: SettingsProfile,
) -> Result<SettingsSaved, String> {
    let profile = save(store, id, profile).await?;
    let sync_warning = notification.notify(id, &profile).err();
    Ok(SettingsSaved {
        profile,
        sync_warning,
    })
}
/// 重試只讀取已儲存設定並通知，不重新保存或建立生活窗。
pub async fn resynchronize(
    store: &impl PetSettings,
    notification: &impl ProfileNotification,
    id: &str,
) -> Result<SettingsSaved, String> {
    let profile = load(store, id).await?;
    let sync_warning = notification.notify(id, &profile).err();
    Ok(SettingsSaved {
        profile,
        sync_warning,
    })
}

pub async fn save_checked_and_notify(
    store: &impl PetSettings,
    notification: &impl ProfileNotification,
    id: &str,
    profile: SettingsProfile,
    expected: SettingsProfile,
) -> Result<SettingsSaved, String> {
    let profile = normalize(profile)?;
    let profile = store.save_checked(id, &profile, &expected).await?;
    Ok(SettingsSaved {
        sync_warning: notification.notify(id, &profile).err(),
        profile,
    })
}
