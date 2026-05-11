use crate::models::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_with::{json::JsonString, serde_as};

#[derive(Serialize, Debug, Clone)]
pub(super) struct UserChatMessageRequest {
    pub model: String,
    pub messages: Vec<ChatMessageRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub(super) struct ChatMessageRequest {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallRequestMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: Option<serde_json::Value>,
    pub strict: Option<bool>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct ToolCallRequestFunction {
    pub index: usize,
    pub name: String,
    #[serde_as(as = "JsonString")]
    pub arguments: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct ToolCallRequestMessage {
    pub id: String,
    #[serde(
        rename(serialize = "type", deserialize = "type"),
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_type: Option<String>,
    pub function: ToolCallRequestFunction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct ResponseFormat {
    pub kind: String,
    pub json_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ModelResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub system_fingerprint: String,
    pub choices: Vec<Choice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_groq: Option<XGroq>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Usage {
    pub queue_time: f64,
    pub prompt_tokens: usize,
    pub prompt_time: f64,
    pub completion_tokens: usize,
    pub completion_time: f64,
    pub total_tokens: usize,
    pub total_time: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum FinishReason {
    Stop,
    Length,
    #[serde(rename = "tool_calls")]
    ToolCalls,
    #[serde(rename = "function_call")]
    FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Choice {
    pub index: usize,
    pub delta: Option<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Message {
    pub role: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub reasoning: Option<String>,
    pub channel: Option<String>,
    pub thinking: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ToolCall {
    pub function: ToolCallFunction,
    pub id: String,
    #[serde(rename(serialize = "type", deserialize = "type"))]
    pub tool_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

impl Into<crate::models::tool::ToolCallRequest> for ToolCall {
    fn into(self) -> crate::models::tool::ToolCallRequest {
        let mut result = crate::models::tool::ToolCallRequest {
            id: self.id,
            tool_type: Some("function".to_string()),
            function: crate::models::tool::ToolCallRequestFunction {
                index: 0, // The index can be set based on your requirements
                name: self.function.name,
                arguments: serde_json::Value::Null, // Set to null or handle as needed
            },
            error: None,
        };
        match serde_json::from_str::<serde_json::Value>(&self.function.arguments) {
            Ok(args) => {
                result.function.arguments = args;
            }
            Err(e) => {
                result.error = Some(format!("Failed to parse arguments as JSON: {}", e));
            }
        }
        result
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct XGroq {
    pub id: String,
}
