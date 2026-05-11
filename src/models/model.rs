use crate::models::tool::ToolCallRequest;

use super::thread::Thread;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/////////////////////////////
// Model trait
/////////////////////////////
#[async_trait]
pub trait LLMModel {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    async fn generate(&self, prompt: &Thread, resp_tx: broadcast::Sender<ChatMessageResponse>);
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}
impl Default for Role {
    fn default() -> Self {
        Role::User
    }
}

impl Role {
    pub fn to_string(&self) -> String {
        match self {
            Role::System => "system".to_string(),
            Role::User => "user".to_string(),
            Role::Assistant => "assistant".to_string(),
            Role::Tool => "tool".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Choice {
    pub index: usize,
    pub message: ChatMessageResponse,
    pub logprobs: Option<serde_json::Value>,
    pub finish_reason: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Usage {
    queue_time: f64,
    prompt_tokens: usize,
    prompt_time: f64,
    completion_tokens: usize,
    completion_time: f64,
    total_tokens: usize,
    total_time: f64,
}

////////////////////
// Model unified output formats
///////////////////

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ChatMessageResponse {
    pub role: String,
    pub content: Option<String>,
    pub thinking: Option<String>,
    pub done: bool,
    pub tool_calls: Option<Vec<ToolCallRequest>>,
    pub error: Option<String>,
}
