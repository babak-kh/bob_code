use crate::models::{
    thread::Thread,
    tool::{Tool, ToolCallRequest},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug, Clone)]
pub(super) struct UserChatMessageRequest {
    pub model: String,
    pub messages: Vec<ChatMessageRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    pub stream: bool,
    pub think: bool,
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
    pub tool_calls: Option<Vec<ToolCallRequest>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct ResponseFormat {
    pub kind: String,
    pub json_schema: Option<serde_json::Value>,
}

impl From<&Thread> for UserChatMessageRequest {
    fn from(val: &Thread) -> Self {
        let result = UserChatMessageRequest {
            model: "gemma4:e4b".to_string(),
            messages: val
                .get_context()
                .iter()
                .flat_map(|m| {
                    let mut data = vec![];
                    if let Some(content) = &m.content {
                        data.push(ChatMessageRequest {
                            role: m.role.to_string(),
                            content: Some(content.clone()),
                            ..Default::default()
                        })
                    };
                    if m.response.is_some() {
                        data.push(ChatMessageRequest {
                            role: "assistant".to_string(),
                            content: m.response.clone(),
                            ..Default::default()
                        })
                    }
                    if let Some(resp) = &m.tool_response {
                        data.push(ChatMessageRequest {
                            role: "tool".to_string(),
                            content: Some(resp.result.clone()),
                            ..Default::default()
                        })
                    }
                    if let Some(tools) = &m.tools {
                        data.push(ChatMessageRequest {
                            role: "assistant".to_string(),
                            tool_calls: Some(tools.clone()),
                            ..Default::default()
                        })
                    }
                    data
                })
                .collect(),
            response_format: None,
            tools: None,
            stream: true,
            think: false,
            keep_alive: None,
            temperature: Some(1.0),
            format: None,
        };
        tracing::info!(
            "Converted thread into UserChatMessageRequest: {:?}",
            serde_json::to_string_pretty(&result).unwrap()
        );
        result
    }
}
