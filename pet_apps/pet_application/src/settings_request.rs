#[derive(Clone, Default, Debug)]
pub struct SettingsRequest {
    generation: u64,
    target: Option<String>,
}
impl SettingsRequest {
    pub fn begin(&mut self, target: Option<String>) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.target = target;
        self.generation
    }
    pub fn accepts(&self, generation: u64, selected: &Option<String>, id: &str) -> bool {
        self.generation == generation
            && self.target.as_deref() == Some(id)
            && selected.as_deref() == Some(id)
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_and_cross_pet_replies_are_rejected() {
        let mut request = SettingsRequest::default();
        let first = request.begin(Some("a".into()));
        assert!(request.accepts(first, &Some("a".into()), "a"));
        let second = request.begin(Some("b".into()));
        assert!(!request.accepts(first, &Some("b".into()), "a"));
        assert!(!request.accepts(second, &Some("a".into()), "b"));
        assert!(request.accepts(second, &Some("b".into()), "b"));
        request.begin(None);
        assert!(!request.accepts(second, &None, "b"));
    }
}
