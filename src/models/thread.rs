use serde::{Deserialize, Serialize};

use crate::{
    models::{
        model::{Choice, Role, Usage},
        tool::{ToolCallRequest, ToolCallResponse},
    },
    tool::{create_file_tool, edit_file_tool, fd_tool, list_files_tool, read_tool, rg_tool},
};

///////////////////
// Context
///////////////////
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ContextItem {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolCallRequest>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_response: Option<ToolCallResponse>,
}
impl ContextItem {
    pub fn new(role: Role, content: Option<String>) -> Self {
        ContextItem {
            role: role,
            content: content,
            ..Default::default()
        }
    }
    pub fn user(content: String) -> Self {
        ContextItem::new(Role::User, Some(content))
    }
    pub fn tool_request(tools: Vec<ToolCallRequest>) -> Self {
        ContextItem::new(Role::Assistant, None).with_tools_req(tools)
    }
    pub fn system(content: String) -> Self {
        ContextItem::new(Role::System, Some(content))
    }
    pub fn assistant(response: String) -> Self {
        ContextItem::new(Role::Assistant, None).with_response(response)
    }
    pub fn tool_response(tool_response: ToolCallResponse) -> Self {
        ContextItem::new(Role::Tool, None).with_tool_response(Some(tool_response))
    }
    fn with_tools_req(mut self, tools: Vec<ToolCallRequest>) -> Self {
        self.tools = Some(tools);
        self
    }
    fn with_response(mut self, response: String) -> Self {
        self.response = Some(response);
        self
    }
    fn with_tool_response(mut self, tool_response: Option<ToolCallResponse>) -> Self {
        self.tool_response = tool_response;
        self
    }
}

///////////////////
// Thread
///////////////////

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub context: Vec<ContextItem>,
}

impl Thread {
    pub fn new(id: String) -> Self {
        Thread {
            context: vec![],
            id,
        }
    }
    pub fn get_context(&self) -> Vec<ContextItem> {
        self.context.clone()
    }
    pub fn get_history_as_string(&self) -> String {
        serde_json::to_string(&self.context.clone()).unwrap_or("[]".to_string())
    }
    pub fn add_context(&mut self, item: ContextItem) {
        self.context.push(item);
    }
}

/*
{"model":"Gemma4:e2b","created_at":"2026-04-29T12:50:47.882318949Z","message":{"role":"assistant","content":"","tool_calls":[{"id":"call_4jviog4l","function":{"index":0,"name":"read","arguments":{"line_from":1,"line_to":1000,"path":"to_read.txt"}}}]},"done":false}
*/

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AgentChatMessageResponse {
    pub id: String,
    pub object: String,
    pub model: String,
    pub created: i64,
    pub choices: Vec<Choice>,
    pub usage: Usage,
    pub system_fingerprint: String,
    pub x_groq: Option<serde_json::Value>,
    pub service_tier: Option<String>,

    pub total_duration: Option<i64>,
    pub load_duration: Option<usize>,
    pub prompt_eval_count: Option<usize>,
    pub prompt_eval_duration: Option<usize>,
    pub eval_count: Option<usize>,
    pub eval_duration: Option<usize>,

    pub done: Option<bool>,
}
