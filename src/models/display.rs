use crate::models::tool::{ToolCallRequest, ToolCallResponse};

/// Discriminates the visual role of a message segment in the response area.
#[derive(Clone, Debug, PartialEq)]
pub enum MessageKind {
    /// Text the user typed and submitted.
    User,
    /// Visible content returned by the assistant.
    AssistantContent,
    /// Internal thinking/reasoning trace (shown dimmed, collapsible).
    AssistantThinking,
    /// Tool call request or response payload (shown dimmed, collapsible).
    AssistantToolCall,
    /// Output from a slash-command (e.g. `/tree`).
    InfoCommandOutput,
}

/// Public data contract passed from `app.rs` into the response area.
/// The controller converts these into [`crate::components::message_block::MessageBlock`]
/// internally; this type stays lean and dependency-free for the call sites.
#[derive(Clone, Debug)]
pub struct ResponseAreaInput {
    pub kind: MessageKind,
    pub content: String,
}

impl ResponseAreaInput {
    pub fn user(content: String) -> Self {
        Self {
            kind: MessageKind::User,
            content,
        }
    }

    pub fn assistant_content(content: String) -> Self {
        Self {
            kind: MessageKind::AssistantContent,
            content,
        }
    }

    pub fn assistant_thinking(content: String) -> Self {
        Self {
            kind: MessageKind::AssistantThinking,
            content,
        }
    }

    pub fn tool_call(req: Vec<ToolCallRequest>) -> Self {
        let content = serde_json::to_string_pretty(&req)
            .unwrap_or_else(|_| "Failed to serialize tool call".to_string());
        Self {
            kind: MessageKind::AssistantToolCall,
            content,
        }
    }

    pub fn tool_response(resp: ToolCallResponse) -> Self {
        Self {
            kind: MessageKind::AssistantToolCall,
            content: serde_json::to_string_pretty(&resp)
                .unwrap_or_else(|_| "Failed to serialize tool response".to_string()),
        }
    }

    pub fn info_command_output(content: String) -> Self {
        Self {
            kind: MessageKind::InfoCommandOutput,
            content,
        }
    }
}
