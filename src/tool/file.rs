use std::io;
use std::path::Path;

use tokio::io::{AsyncBufReadExt, BufReader};

use crate::models::tool::{DiffViewData, Tool, ToolFunction};

pub fn read_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: ToolFunction {
            name: "read_file".to_string(),
            description: "partial file read based on line_from and line_to parameters. \
                keep line_to rather small number so that parts of file needed is fetched. \
                if line_to is greater than file lines, line_from to end of file is returned"
                .to_string(),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to read"
                    },
                    "line_from": {
                        "type": "integer",
                        "description": "Starting line number to read from"
                    },
                    "line_to": {
                        "type": "integer",
                        "description": "Ending line number to read to."
                    },
                },
                "additionalProperties": false,
                "required": ["path", "line_from", "line_to"]
            })),
            strict: Some(true),
        },
    }
}

#[derive(Debug)]
pub(super) enum ReadError {
    Io(io::Error),
    InvalidRange { from: usize, to: usize },
    LineOutOfBounds { requested: usize, total: usize },
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Io(e) => write!(f, "IO error: {e}"),
            ReadError::InvalidRange { from, to } => {
                write!(f, "Invalid range: 'from' ({from}) must be <= 'to' ({to})")
            }
            ReadError::LineOutOfBounds { requested, total } => {
                write!(f, "Line {requested} out of bounds, file has {total} lines")
            }
        }
    }
}

impl From<io::Error> for ReadError {
    fn from(e: io::Error) -> Self {
        ReadError::Io(e)
    }
}

pub(super) async fn read_lines_range(
    path: &str,
    from: usize,
    to: usize,
) -> Result<Vec<String>, ReadError> {
    // from > to is always invalid
    if from > to {
        return Err(ReadError::InvalidRange { from, to });
    }

    // from == 0 makes no sense for 1-based line numbers
    if from == 0 {
        return Err(ReadError::InvalidRange { from, to });
    }

    let file = tokio::fs::File::open(path).await?;
    let reader = BufReader::new(file);
    let mut lines_iter = reader.lines();

    let mut result = Vec::new();
    let mut line_num: usize = 0;

    while let Some(line) = lines_iter.next_line().await? {
        line_num += 1;

        if line_num >= from && line_num <= to {
            result.push(line);
        }

        if line_num >= to {
            break; // collected everything we need
        }
    }

    if from > line_num {
        return Err(ReadError::LineOutOfBounds {
            requested: from,
            total: line_num,
        });
    }

    // If `to` exceeds the file length we already collected lines up to EOF — just return them.
    Ok(result)
}

pub fn create_file_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: ToolFunction {
            name: "create_file".to_string(),
            description: "Create a new file at the given path with the provided content. \
                Parent directories are created automatically if they do not exist. \
                Overwrites the file if it already exists."
                .to_string(),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path of the file to create"
                    },
                    "content": {
                        "type": "string",
                        "description": "Full text content to write into the file"
                    }
                },
                "additionalProperties": false,
                "required": ["path", "content"],
            })),
            strict: Some(true),
        },
    }
}

pub fn edit_file_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: ToolFunction {
            name: "edit_file".to_string(),
            description: "Edit an existing file by replacing an exact occurrence of old_text \
                with new_text. old_text must match exactly once in the file — the call fails \
                if it is not found or appears more than once. \
                Use read_file first to obtain the exact text to match."
                .to_string(),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to edit"
                    },
                    "old_text": {
                        "type": "string",
                        "description": "Exact text to search for in the file. Must appear exactly once."
                    },
                    "new_text": {
                        "type": "string",
                        "description": "Text to replace old_text with"
                    }
                },
                "additionalProperties": false,
                "required": ["path", "old_text", "new_text"]
            })),
            strict: Some(true),
        },
    }
}

pub fn list_files_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: ToolFunction {
            name: "list_files_and_directories".to_string(),
            description: "List all files and directories in a given directory. \
                directories are shown with \"\\\" at the end of name"
                .to_string(),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the directory to list files from"
                    }
                },
                "additionalProperties": false,
                "required": ["path"]
            })),
            strict: Some(true),
        },
    }
}

pub(super) async fn create_file(path: &str, content: &str) -> Result<String, io::Error> {
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }
    tokio::fs::write(path, content).await?;
    Ok(format!("Successfully created '{path}'"))
}

#[derive(Debug)]
pub(super) enum EditError {
    Io(io::Error),
    NotFound,
    Ambiguous(usize),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::Io(e) => write!(f, "IO error: {e}"),
            EditError::NotFound => write!(f, "old_text not found in file"),
            EditError::Ambiguous(n) => {
                write!(f, "old_text matches {n} locations — make it more specific")
            }
        }
    }
}

impl From<io::Error> for EditError {
    fn from(e: io::Error) -> Self {
        EditError::Io(e)
    }
}

pub(super) async fn edit_file(
    path: &str,
    old_text: &str,
    new_text: &str,
) -> Result<(String, Option<crate::models::tool::DiffViewData>), EditError> {
    let content = tokio::fs::read_to_string(path).await?;
    let count = content.matches(old_text).count();
    match count {
        0 => return Err(EditError::NotFound),
        n if n > 1 => return Err(EditError::Ambiguous(n)),
        _ => {}
    }

    let new_content = content.replacen(old_text, new_text, 1);

    // Compute diff between the old and new content *before* writing.
    let diff = compute_diff(path, &content, &new_content);

    tokio::fs::write(path, &new_content).await?;
    Ok((format!("Successfully edited '{path}'"), diff))
}

/// Compute a simple line-based unified diff between `old` and `new`.
fn compute_diff(path: &str, old: &str, new: &str) -> Option<DiffViewData> {
    use crate::models::tool::{DiffHunk, DiffLine, DiffViewData};

    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut old_i = 0usize;
    let mut new_i = 0usize;

    while old_i < old_lines.len() || new_i < new_lines.len() {
        // Skip matching prefix.
        let mut skip = 0;
        while old_i + skip < old_lines.len()
            && new_i + skip < new_lines.len()
            && old_lines[old_i + skip] == new_lines[new_i + skip]
        {
            skip += 1;
        }
        if skip > 0 {
            old_i += skip;
            new_i += skip;
        }

        if old_i >= old_lines.len() && new_i >= new_lines.len() {
            break;
        }

        // We found a mismatch region — look ahead to find matching suffix.
        let mut del = 0;
        let mut ins = 0;
        // Simple O(N*M) scan to find the next common line.
        'outer: for d in 0..=old_lines.len().saturating_sub(old_i) {
            for i in 0..=new_lines.len().saturating_sub(new_i) {
                if old_i + d < old_lines.len()
                    && new_i + i < new_lines.len()
                    && old_lines[old_i + d] == new_lines[new_i + i]
                {
                    del = d;
                    ins = i;
                    break 'outer;
                }
            }
        }
        if del == 0 && ins == 0 {
            // Reached end with pure additions or deletions.
            del = old_lines.len().saturating_sub(old_i);
            ins = new_lines.len().saturating_sub(new_i);
        }

        let mut lines: Vec<DiffLine> = Vec::new();

        // Context lines before the change (up to 3).
        let ctx_start = old_i.saturating_sub(3.min(old_i));
        let mut real_old_start = old_i + 1; // 1-based
        if ctx_start < old_i {
            for k in ctx_start..old_i {
                lines.push(DiffLine::Context(old_lines[k].to_string()));
            }
            real_old_start = ctx_start + 1;
        }

        for k in 0..del {
            lines.push(DiffLine::Removed(old_lines[old_i + k].to_string()));
        }
        for k in 0..ins {
            lines.push(DiffLine::Added(new_lines[new_i + k].to_string()));
        }

        // Context lines after the change (up to 3).
        let ctx_end = (old_i + del + 3).min(old_lines.len());
        for k in (old_i + del)..ctx_end {
            lines.push(DiffLine::Context(old_lines[k].to_string()));
        }

        hunks.push(DiffHunk {
            old_start: real_old_start,
            new_start: new_i.saturating_sub(old_i.saturating_sub(real_old_start)) + 1,
            lines,
        });

        old_i += del;
        new_i += ins;
    }

    if hunks.is_empty() {
        None
    } else {
        Some(DiffViewData {
            file_path: path.to_string(),
            hunks,
        })
    }
}

pub(super) async fn list_files_and_directory(path: &str) -> Result<Vec<String>, io::Error> {
    let mut files = Vec::new();
    let mut dir = tokio::fs::read_dir(path).await?;
    while let Some(entry) = dir.next_entry().await? {
        let file_type = entry.file_type().await?;
        if file_type.is_file() {
            files.push(entry.file_name().to_string_lossy().to_string());
        } else if file_type.is_dir() {
            files.push(format!("{}\\", entry.file_name().to_string_lossy()));
        }
    }
    Ok(files)
}
