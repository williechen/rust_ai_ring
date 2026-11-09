pub mod login_input;
pub mod message_rules;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessageAuthor {
    AnonymousUser,
    User { user_id: String },
    Ai { persona_id: String },
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub room_id: String,
    pub author: MessageAuthor,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Room {
    pub id: String,
    pub category_room_id: String,
    pub category_label: Option<String>,
    pub name: String,
}
