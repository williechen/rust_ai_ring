use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRequest {
    pub message: String,
}

impl ChatRequest {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatResponse {
    pub text: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AiError {
    #[error("chat message cannot be empty")]
    EmptyMessage,

    #[error("model request failed: {0}")]
    Model(String),
}

#[async_trait]
pub trait ChatModel: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, AiError>;
}

#[derive(Debug, Default)]
pub struct MockChatModel;

#[async_trait]
impl ChatModel for MockChatModel {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, AiError> {
        let message = request.message.trim();

        if message.is_empty() {
            return Err(AiError::EmptyMessage);
        }

        Ok(ChatResponse {
            text: format!("mock: {message}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[tokio::test]
    async fn mock_chat_returns_response() {
        let model = MockChatModel;

        let response = model
            .chat(ChatRequest::new("你好"))
            .await
            .expect("mock chat should succeed");

        assert_eq!(response.text, "mock: 你好");
    }

    #[tokio::test]
    async fn empty_message_is_rejected() {
        let model = MockChatModel;

        let error = model
            .chat(ChatRequest::new("   "))
            .await
            .expect_err("empty message should fail");

        assert_eq!(error, AiError::EmptyMessage);
    }

    #[tokio::test]
    async fn chat_model_can_be_used_as_trait_object() {
        let model: Arc<dyn ChatModel> = Arc::new(MockChatModel);

        let response = model
            .chat(ChatRequest::new("hello"))
            .await
            .expect("trait object should work");

        assert_eq!(response.text, "mock: hello");
    }
}
