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
