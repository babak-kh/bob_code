use crate::{
    models::{
        thread::Thread,
        tool::{Tool, ToolCallRequest},
    },
    tool::{create_file_tool, edit_file_tool, fd_tool, list_files_tool, read_tool, rg_tool},
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

impl Into<UserChatMessageRequest> for &Thread {
    fn into(self) -> UserChatMessageRequest {
        let result = UserChatMessageRequest {
            model: "gemma4:e4b".to_string(),
            messages: self
                .get_context()
                .iter()
                .map(|m| {
                    let mut data = vec![];
                    if let Some(content) = &m.content {
                        data.push(ChatMessageRequest {
                            role: m.role.to_string(),
                            content: Some(content.clone()),
                            ..Default::default()
                        })
                    };
                    if !m.response.is_none() {
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
                .flatten()
                .collect(),
            response_format: None,
            tools: Some(vec![
                read_tool(),
                list_files_tool(),
                create_file_tool(),
                edit_file_tool(),
                fd_tool(),
                rg_tool(),
            ]),
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
