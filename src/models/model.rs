use std::fmt::Display;

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

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub enum Role {
    System,
    #[default]
    User,
    Assistant,
    Tool,
}

impl Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s: &str = "user";
        match self {
            Role::User => (),
            Role::System => s = "system",
            Role::Assistant => s = "assistant",
            Role::Tool => s = "tool",
        }
        write!(f, "{}", s)
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
