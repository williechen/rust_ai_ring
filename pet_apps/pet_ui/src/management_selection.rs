use crate::bridge::SavedPet;
use leptos::prelude::*;

/// 控制窗的編輯對象是暫存UI選取，不能改動生活窗或活動持久狀態。
pub fn choose_management_pet(
    id: &str,
    pets: &[SavedPet],
    selected: WriteSignal<Option<String>>,
) -> Result<(), String> {
    if !pets.iter().any(|p| p.id == id) {
        return Err("請重新載入清單並選擇存在的寵物".into());
    }
    selected.set(Some(id.to_owned()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::SavedPet;
    use leptos::prelude::*;
    #[test]
    fn control_selection_uses_existing_pet_and_invalid_id_preserves_previous() {
        Owner::new().with(|| {
            let pets = vec![
                SavedPet {
                    id: "a".into(),
                    name: "第一隻".into(),
                    species: "tortoise".into(),
                },
                SavedPet {
                    id: "b".into(),
                    name: "第二隻".into(),
                    species: "tortoise".into(),
                },
            ];
            let original = pets.clone();
            let (selected, set_selected) = signal(None::<String>);
            assert!(choose_management_pet("a", &pets, set_selected).is_ok());
            assert_eq!(selected.get_untracked().as_deref(), Some("a"));
            assert!(choose_management_pet("b", &pets, set_selected).is_ok());
            assert_eq!(selected.get_untracked().as_deref(), Some("b"));
            assert!(choose_management_pet("missing", &pets, set_selected).is_err());
            assert_eq!(selected.get_untracked().as_deref(), Some("b"));
            assert_eq!(
                pets.iter().map(|p| (&p.id, &p.name)).collect::<Vec<_>>(),
                original
                    .iter()
                    .map(|p| (&p.id, &p.name))
                    .collect::<Vec<_>>()
            );
        });
    }
}
