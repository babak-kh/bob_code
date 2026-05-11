use std::io;

use tokio::process::Command;

use crate::models::tool::{Tool, ToolFunction};

pub fn fd_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: ToolFunction {
            name: "fd_search".to_string(),
            description: "Search for files and directories by name using fd (fd-find). \
                Returns matching paths one per line. \
                Use 'type' to filter: 'f' = files only, 'd' = directories only, 'l' = symlinks only. \
                Omit 'type' to match all entry kinds. \
                Omit 'path' to search from the current working directory."
                .to_string(),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "The name pattern to search for (regex)"
                    },
                    "path": {
                        "type": "string",
                        "description": "Root directory to search in. Defaults to '.' (current directory)"
                    },
                    "type": {
                        "type": "string",
                        "description": "Entry type filter: 'f' for files, 'd' for directories, 'l' for symlinks",
                        "enum": ["f", "d", "l"]
                    }
                },
                "additionalProperties": false,
                "required": ["pattern"]
            })),
            strict: Some(false),
        },
    }
}

pub fn rg_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: ToolFunction {
            name: "rg_search".to_string(),
            description: "Search file contents using ripgrep (rg). \
                Returns matching lines prefixed with their file path and line number. \
                Optionally include N surrounding context lines above and below each match. \
                Omit 'path' to search from the current working directory."
                .to_string(),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "The regex pattern to search for inside file contents",
                    },
                    "path": {
                        "type": "string",
                        "description": "File or directory to search in. Defaults to '.' (current directory)"
                    },
                    "context_lines": {
                        "type": "integer",
                        "description": "Number of lines of context to show around each match (passed as -C to rg)"
                    }
                },
                "additionalProperties": false,
                "required": ["pattern"]
            })),
            strict: Some(false),
        },
    }
}

pub(super) async fn fd_search(
    pattern: &str,
    path: &str,
    kind: Option<&str>,
) -> Result<String, io::Error> {
    let mut cmd = Command::new("fd");
    cmd.arg(pattern);
    cmd.arg(path);
    if let Some(k) = kind {
        cmd.args(["--type", k]);
    }
    let output = cmd.output().await?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if stdout.is_empty() {
        Ok("No matches found.".to_string())
    } else {
        Ok(stdout)
    }
}

pub(super) async fn rg_search(
    pattern: &str,
    path: &str,
    context_lines: Option<i64>,
) -> Result<String, io::Error> {
    let mut cmd = Command::new("rg");
    cmd.arg("--line-number");
    cmd.arg("--color=never");
    if let Some(n) = context_lines {
        cmd.args(["-C", &n.to_string()]);
    }
    cmd.arg(pattern);
    cmd.arg(path);
    let output = cmd.output().await?;
    // rg exits with code 1 when there are no matches — that is not an error
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if stdout.is_empty() {
        Ok("No matches found.".to_string())
    } else {
        Ok(stdout)
    }
}
