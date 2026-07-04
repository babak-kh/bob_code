use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolCallRequestFunction {
    pub index: usize,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Tool {
    #[serde(rename(serialize = "type"))]
    pub tool_type: String,
    pub function: ToolFunction,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolCallResponse {
    pub id: String,
    pub result: String,
    /// Optional structured output for rich TUI rendering.
    /// Not serialized into thread history (display-only).
    #[serde(skip)]
    pub structured: Option<ToolStructuredOutput>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: Option<serde_json::Value>,
    pub strict: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolCallRequest {
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
pub struct ToolCatalogEntry {
    pub name: String,
    pub description: String,
}

// ── Tool execution result ─────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolResult {
    /// Plain-text representation (always present, used for the LLM context).
    pub text: String,
    /// Optional structured output for rich TUI rendering.
    pub structured: Option<ToolStructuredOutput>,
}

/// Kinds of structured output a tool can produce for the response area.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ToolStructuredOutput {
    DiffView(DiffViewData),
}

/// Data needed to render a file diff.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiffViewData {
    pub file_path: String,
    pub hunks: Vec<DiffHunk>,
}

/// A contiguous region of changed lines.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiffHunk {
    /// 1-based starting line in the old file.
    pub old_start: usize,
    /// 1-based starting line in the new file.
    pub new_start: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
}
