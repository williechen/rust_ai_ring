use leptos::prelude::*;

use crate::saved_pet::SavedPet;

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

/// 清單更新後移除不存在的管理對象；不觸發出籠或任何 IPC。
pub fn reconcile_management_pet(
    pets: &[SavedPet],
    selected: ReadSignal<Option<String>>,
    set_selected: WriteSignal<Option<String>>,
) -> bool {
    let stale = selected
        .get_untracked()
        .is_some_and(|id| !pets.iter().any(|p| p.id == id));
    if stale {
        set_selected.set(None);
    }
    stale
}

#[cfg(test)]
mod tests {
    use super::*;
    use leptos::prelude::*;

    #[test]
    fn refresh_clears_only_missing_management_selection() {
        Owner::new().with(|| {
            let rows = vec![SavedPet {
                id: "b".into(),
                name: "第二隻".into(),
                species: "tortoise".into(),
            }];
            let (selected, set_selected) = signal(Some("a".to_string()));
            assert!(reconcile_management_pet(&rows, selected, set_selected));
            assert_eq!(selected.get_untracked(), None);
            set_selected.set(Some("b".into()));
            assert!(!reconcile_management_pet(&rows, selected, set_selected));
            assert_eq!(selected.get_untracked().as_deref(), Some("b"));
        });
    }

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
