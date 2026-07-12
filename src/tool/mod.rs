pub use crate::models::tool::{ToolCallRequest, ToolResult, ToolStructuredOutput};
pub use bash::{bash_tool, bash_tool_run};
pub use file::{create_file_tool, edit_file_tool, list_files_tool, read_tool};
pub use http::{http_request_exec, http_request_tool};
pub use search::{fd_tool, rg_tool};

pub mod bash;
pub mod file;
pub mod http;
pub mod search;

use crate::models::tool::{Tool, ToolCatalogEntry};
use file::{create_file, edit_file, list_files_and_directory, read_lines_range};
use search::{fd_search, rg_search};

pub async fn execute_tool(call: &ToolCallRequest) -> ToolResult {
    match call.function.name.as_str() {
        "read_file" => {
            let path = call.function.arguments["path"].as_str().unwrap_or("");
            let line_from = call.function.arguments["line_from"].as_i64().unwrap_or(0);
            let line_to = call.function.arguments["line_to"].as_i64().unwrap_or(0);
            let text = read_lines_range(path, line_from as usize, line_to as usize)
                .await
                .map(|lines| lines.join("\n"))
                .unwrap_or_else(|e| format!("Error executing read tool: {e}"));
            ToolResult {
                text,
                structured: None,
            }
        }
        "list_files_and_directories" => {
            let path = call.function.arguments["path"].as_str().unwrap_or("");
            let text = match list_files_and_directory(path).await {
                Ok(files) => files.join("\n"),
                Err(e) => format!("Error executing list_files tool: {e}"),
            };
            ToolResult {
                text,
                structured: None,
            }
        }
        "create_file" => {
            let path = call.function.arguments["path"].as_str().unwrap_or("");
            let content = call.function.arguments["content"].as_str().unwrap_or("");
            let text = create_file(path, content)
                .await
                .unwrap_or_else(|e| format!("Error creating file: {e}"));
            ToolResult {
                text,
                structured: None,
            }
        }
        "edit_file" => {
            let path = call.function.arguments["path"].as_str().unwrap_or("");
            let old_text = call.function.arguments["old_text"].as_str().unwrap_or("");
            let new_text = call.function.arguments["new_text"].as_str().unwrap_or("");
            let (text, diff) = match edit_file(path, old_text, new_text).await {
                Ok((msg, diff)) => (msg, diff),
                Err(e) => (format!("Error editing file: {e}"), None),
            };
            ToolResult {
                text,
                structured: diff.map(ToolStructuredOutput::DiffView),
            }
        }
        "fd_search" => {
            let pattern = call.function.arguments["pattern"].as_str().unwrap_or("");
            let path = call.function.arguments["path"].as_str().unwrap_or(".");
            let kind = call.function.arguments["type"].as_str();
            let text = fd_search(pattern, path, kind)
                .await
                .unwrap_or_else(|e| format!("Error running fd: {e}"));
            ToolResult {
                text,
                structured: None,
            }
        }
        "rg_search" => {
            let pattern = call.function.arguments["pattern"].as_str().unwrap_or("");
            let path = call.function.arguments["path"].as_str().unwrap_or(".");
            let context_lines = call.function.arguments["context_lines"].as_i64();
            let text = rg_search(pattern, path, context_lines)
                .await
                .unwrap_or_else(|e| format!("Error running rg: {e}"));
            ToolResult {
                text,
                structured: None,
            }
        }
        "bash" => {
            let Some(cmd) = call.function.arguments["command"].as_str() else {
                return ToolResult {
                    text: "command argument is mandatory".to_string(),
                    structured: None,
                };
            };

            let args: Vec<String> = call.function.arguments["args"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| v.to_string())
                .collect();

            let result = bash_tool_run(cmd.to_string(), args)
                .await
                .unwrap_or_else(|e| format!("Error running bash: {e}"));
            ToolResult {
                text: result,
                structured: None,
            }
        }
        "http_request" => {
            let url = call.function.arguments["url"].as_str().unwrap_or("");
            let method = call.function.arguments["method"].as_str().unwrap_or("GET");
            let headers = call.function.arguments.get("headers");
            let body = call.function.arguments["body"].as_str();
            let timeout_secs = call.function.arguments["timeout_seconds"].as_u64();
            let text = http_request_exec(url, method, headers, body, timeout_secs).await;
            ToolResult {
                text,
                structured: None,
            }
        }
        _ => ToolResult {
            text: format!("Unknown tool: {:?}", call.function.name),
            structured: None,
        },
    }
}

pub fn tools_catalog() -> Vec<ToolCatalogEntry> {
    vec![
        ToolCatalogEntry {
            name: "read_file".to_string(),
            description: "Read a range of lines from a file.".to_string(),
        },
        ToolCatalogEntry {
            name: "list_files_and_directories".to_string(),
            description: "List files and directories at a path.".to_string(),
        },
        ToolCatalogEntry {
            name: "create_file".to_string(),
            description: "Create a new file with given content (overwrites if exists).".to_string(),
        },
        ToolCatalogEntry {
            name: "edit_file".to_string(),
            description: "Replace an exact unique occurrence of old_text with new_text in a file."
                .to_string(),
        },
        ToolCatalogEntry {
            name: "fd_search".to_string(),
            description: "Search for files and directories by name using fd-find.".to_string(),
        },
        ToolCatalogEntry {
            name: "rg_search".to_string(),
            description: "Search file contents using ripgrep.".to_string(),
        },
        ToolCatalogEntry {
            name: "bash".to_string(),
            description: "Run bash command on OS. command and arguments are passed to bash -c"
                .to_string(),
        },
        ToolCatalogEntry {
            name: "http_request".to_string(),
            description: "Perform an HTTP request to a public URL. Supports GET/POST/PUT/DELETE. Internal/private IPs are blocked."
                .to_string(),
        },
    ]
}

pub fn default_tools() -> Vec<Tool> {
    vec![
        read_tool(),
        list_files_tool(),
        create_file_tool(),
        edit_file_tool(),
        fd_tool(),
        rg_tool(),
        bash_tool(),
        http_request_tool(),
    ]
}
