pub mod auth;

use chat_domain::ChatMessage;
use serde::{Deserialize, Serialize};

/*{
  "type": "send_message",
  "data": {
    "room_id": "...",
    "content": "Hello"
  }
}*/
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ClientEvent {
    SendMessage { room_id: String, content: String },
    Typing { room_id: String, active: bool },
    Ping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ServerEvent {
    MessageCreated(ChatMessage),
    AiDelta { message_id: String, delta: String },
    AiCompleted { message_id: String },
    PresenceChanged { user_id: String, online: bool },
    Error { code: String, message: String },
    Pong,
    TypingChanged { room_id: String, active: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRoomRequest {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRoomResponse {
    pub id: String,
}

#[cfg(test)]
mod tests {
    use chat_domain::MessageAuthor;

    use super::*;

    #[test]
    fn client_event_json_roundtrip() {
        let event = ClientEvent::SendMessage {
            room_id: "room-rust".to_string(),
            content: String::from("Hello"),
        };
        let serialized = serde_json::to_string(&event).unwrap();
        let deserialized: ClientEvent = serde_json::from_str(&serialized).unwrap();

        assert_eq!(event, deserialized);
    }

    #[test]
    fn ai_message_round_trip() {
        let message = ChatMessage {
            id: "message-001".to_string(),
            room_id: "room-rust".to_string(),
            author: MessageAuthor::Ai {
                persona_id: "persona-rust".to_string(),
            },
            content: "hello".to_string(),
        };

        let event = ServerEvent::MessageCreated(message);

        let json = serde_json::to_string(&event).unwrap();

        let decoded: ServerEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, event);
    }

    #[test]
    fn create_room_http_contract_roundtrip() {
        let request = CreateRoomRequest {
            name: "New Room".to_string(),
        };

        let json = serde_json::to_string(&request).expect("create room request should serialize");

        let decoded: CreateRoomRequest =
            serde_json::from_str(&json).expect("create room request should deserialize");

        assert_eq!(decoded, request);

        let response = CreateRoomResponse {
            id: "room-001".to_string(),
        };

        let json = serde_json::to_string(&response).expect("create room response should serialize");

        let decoded: CreateRoomResponse =
            serde_json::from_str(&json).expect("create room response should deserialize");

        assert_eq!(decoded, response);
    }

    #[test]
    fn typing_changed_roundtrip() {
        let event = ServerEvent::TypingChanged {
            room_id: "room-a".into(),
            active: true,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(serde_json::from_str::<ServerEvent>(&json).unwrap(), event);
    }
}
