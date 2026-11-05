pub use pet_domain::profile::PetProfile as SettingsProfile;
pub trait PetSettings {
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
                name: "
"
                .into(),
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
