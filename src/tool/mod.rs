pub use crate::models::tool::{Tool, ToolCallRequest, ToolFunction};
pub use file::{create_file_tool, edit_file_tool, list_files_tool, read_tool};
pub use search::{fd_tool, rg_tool};

pub mod file;
pub mod search;

use crate::models::tool::ToolCatalogEntry;
use file::{create_file, edit_file, list_files_and_directory, read_lines_range};
use search::{fd_search, rg_search};

pub async fn execute_tool(call: &ToolCallRequest) -> String {
    match call.function.name.as_str() {
        "read_file" => {
            let path = call.function.arguments["path"].as_str().unwrap_or("");
            let line_from = call.function.arguments["line_from"].as_i64().unwrap_or(0);
            let line_to = call.function.arguments["line_to"].as_i64().unwrap_or(0);
            read_lines_range(path, line_from as usize, line_to as usize)
                .await
                .map(|lines| lines.join("\n"))
                .unwrap_or_else(|e| format!("Error executing read tool: {e}"))
        }
        "list_files_and_directories" => {
            let path = call.function.arguments["path"].as_str().unwrap_or("");
            match list_files_and_directory(path).await {
                Ok(files) => files.join("\n"),
                Err(e) => format!("Error executing list_files tool: {e}"),
            }
        }
        "create_file" => {
            let path = call.function.arguments["path"].as_str().unwrap_or("");
            let content = call.function.arguments["content"].as_str().unwrap_or("");
            create_file(path, content)
                .await
                .unwrap_or_else(|e| format!("Error creating file: {e}"))
        }
        "edit_file" => {
            let path = call.function.arguments["path"].as_str().unwrap_or("");
            let old_text = call.function.arguments["old_text"].as_str().unwrap_or("");
            let new_text = call.function.arguments["new_text"].as_str().unwrap_or("");
            edit_file(path, old_text, new_text)
                .await
                .unwrap_or_else(|e| format!("Error editing file: {e}"))
        }
        "fd_search" => {
            let pattern = call.function.arguments["pattern"].as_str().unwrap_or("");
            let path = call.function.arguments["path"].as_str().unwrap_or(".");
            let kind = call.function.arguments["type"].as_str();
            fd_search(pattern, path, kind)
                .await
                .unwrap_or_else(|e| format!("Error running fd: {e}"))
        }
        "rg_search" => {
            let pattern = call.function.arguments["pattern"].as_str().unwrap_or("");
            let path = call.function.arguments["path"].as_str().unwrap_or(".");
            let context_lines = call.function.arguments["context_lines"].as_i64();
            rg_search(pattern, path, context_lines)
                .await
                .unwrap_or_else(|e| format!("Error running rg: {e}"))
        }
        _ => {
            format!("Unknown tool: {:?}", call.function.name)
        }
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
            description: "Replace an exact unique occurrence of old_text with new_text in a file.".to_string(),
        },
        ToolCatalogEntry {
            name: "fd_search".to_string(),
            description: "Search for files and directories by name using fd-find.".to_string(),
        },
        ToolCatalogEntry {
            name: "rg_search".to_string(),
            description: "Search file contents using ripgrep.".to_string(),
        },
    ]
}
