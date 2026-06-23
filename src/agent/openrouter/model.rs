use crate::models::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_with::{json::JsonString, serde_as};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ModelResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    pub choices: Vec<Choice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponseUsage>,
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
pub(super) enum FinishReason {
    #[serde(rename = "stop", alias = "Stop")]
    Stop,
    #[serde(rename = "length", alias = "Length")]
    Length,
    #[serde(rename = "tool_calls", alias = "ToolCalls")]
    ToolCalls,
    #[serde(rename = "function_call", alias = "FunctionCall")]
    FunctionCall,
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
pub(super) struct ResponseFormat {
    pub kind: String,
    pub json_schema: Option<serde_json::Value>,
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

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct ToolCallRequestFunction {
    pub index: usize,
    pub name: String,
    #[serde_as(as = "JsonString")]
    pub arguments: serde_json::Value,
}

/// OpenRouter always returns detailed usage information.
/// Token counts are calculated using the model's native tokenizer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseUsage {
    /// Including images, input audio, and tools if any
    pub prompt_tokens: u32,
    /// The tokens generated
    pub completion_tokens: u32,
    /// Sum of the above two fields
    pub total_tokens: u32,
    /// Breakdown of prompt tokens (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    /// Breakdown of completion tokens (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
    /// Cost in credits (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    /// Whether request used Bring Your Own Key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_byok: Option<bool>,
    /// Detailed cost breakdown (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_details: Option<CostDetails>,
    /// Server-side tool usage (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_tool_use: Option<ServerToolUse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTokensDetails {
    /// Tokens cached by the endpoint
    pub cached_tokens: u32,
    /// Tokens written to cache (models with explicit caching)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u32>,
    /// Tokens used for input audio
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<u32>,
    /// Tokens used for input video
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionTokensDetails {
    /// Tokens generated for reasoning
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
    /// Tokens generated for audio output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<u32>,
    /// Tokens generated for image output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostDetails {
    /// Only shown for BYOK requests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_inference_cost: Option<f64>,
    pub upstream_inference_prompt_cost: f64,
    pub upstream_inference_completions_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerToolUse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search_requests: Option<u32>,
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
