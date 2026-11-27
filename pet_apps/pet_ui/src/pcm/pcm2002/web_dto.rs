use pet_domain::profile::PetProfile;

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct SettingsDraft {
    pub name: String,
    pub species: String,
    pub likes: String,
    pub personality: String,
    pub interests: String,
}

impl From<PetProfile> for SettingsDraft {
    fn from(p: PetProfile) -> Self {
        Self {
            name: p.name,
            species: p.species,
            likes: p.likes.join("\n"),
            personality: p.personality.join("\n"),
            interests: p.interests.join("\n"),
        }
    }
}

fn tags(s: &str) -> Vec<String> {
    s.split([',', '，', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

impl SettingsDraft {
    pub fn profile(self) -> PetProfile {
        PetProfile {
            name: self.name,
            species: self.species,
            likes: tags(&self.likes),
            personality: tags(&self.personality),
            interests: tags(&self.interests),
            appearance: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn draft_identity_includes_each_unsaved_field_and_roundtrips_tags() {
        let saved = SettingsDraft {
            name: "小龜".into(),
            species: "tortoise".into(),
            ..Default::default()
        };
        for field in 0..5 {
            let mut draft = saved.clone();
            match field {
                0 => draft.name.push('甲'),
                1 => draft.species.push('甲'),
                2 => draft.likes.push('甲'),
                3 => draft.personality.push('甲'),
                _ => draft.interests.push('甲'),
            }
            assert_ne!(draft, saved);
        }
        assert_eq!(saved.clone(), saved);
        let mut draft = saved;
        draft.likes = "草，水\n太陽".into();
        let profile = draft.profile();
        assert_eq!(profile.likes, vec!["草", "水", "太陽"]);
        assert_eq!(SettingsDraft::from(profile).likes, "草\n水\n太陽");
    }
}
