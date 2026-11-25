#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdleAction {
    Idle,
    Sleep,
    Sit,
    Walk,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IdleStatus {
    pub pet_id: String,
    pub outside: bool,
    pub action: Option<IdleAction>,
    pub selection_revision: u64,
    pub next_in_seconds: Option<u64>,
}
