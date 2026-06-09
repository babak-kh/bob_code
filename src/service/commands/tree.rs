use crate::models::{model::Role, thread::Thread};

/// Render the full conversation context as a list of human-readable lines.
pub fn render_tree(thread: &Thread) -> Vec<String> {
        let mut result = Vec::new();
        for thread_item in thread.context.iter() {
            match thread_item.role {
                Role::System | Role::User => {
                    if let Some(content) = &thread_item.content {
                        result.push(format!("{}: {}\n", thread_item.role.to_string(), content));
                    } else {
                        result.push(format!(
                            "{}: {}\n",
                            thread_item.role.to_string(),
                            "No content"
                        ));
                    }
                }
                Role::Assistant => {
                    if let Some(response) = &thread_item.response {
                        result.push(format!("Assistant: {}", response));
                    } else {
                        result.push("Assistant: No response".to_string());
                    }
                    if let Some(tools) = &thread_item.tools {
                        for tool in tools.iter() {
                            result.push(format!(
                                "Tool Request: {} - {} - {}",
                                tool.id, tool.function.name, tool.function.arguments,
                            ));
                        }
                    }
                }
                Role::Tool => {
                    if let Some(tool_response) = &thread_item.tool_response {
                        result.push(format!(
                            "Tool: {} - {}",
                            tool_response.id, tool_response.result
                        ));
                    } else {
                        result.push("Tool: No response".to_string());
                    }
                }
            }
        }
    result
}
