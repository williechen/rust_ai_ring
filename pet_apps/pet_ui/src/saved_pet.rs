pub use pet_application::creation::CreatedPet as SavedPet;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_pet_identity_and_name_remain_distinct() {
        let a = SavedPet {
            id: "pet-a".into(),
            name: "第一隻".into(),
            species: "tortoise".into(),
        };
        let mut b = a.clone();
        b.name = "更名".into();
        assert_eq!(a.id, b.id);
        assert_ne!(a.name, b.name);
    }
}
