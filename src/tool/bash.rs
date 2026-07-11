use crate::models::tool::{Tool, ToolFunction};
use std::process::Command;

pub fn bash_tool() -> Tool {
    Tool {
        tool_type: String::from("function"),
        function: ToolFunction {
            name: String::from("bash"),
            description:
                "bash access for commands to run on OS level. other tools should be preffered\
                          before bash access. Access is limited. command and args \
                          are passed to bash -c "
                    .to_string(),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "base command to run",
                    },
                    "args": {
                        "type": "array",
                        "description": "all arguments for the command as an array of strings",
                        "items": {
                            "type": "string"
                        },
                    }
                },
                "additionalProperties": false,
                "required": ["command"],
            })),
            strict: Some(true),
        },
    }
}

pub async fn bash_tool_run(command: String, args: Vec<String>) -> Result<String, String> {
    let mut c = Command::new("bash");
    let out = c
        .arg("-c")
        .arg(command)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    let stout = out.stdout.as_ref();
    Ok(String::from_utf8_lossy(stout).to_string())
}
