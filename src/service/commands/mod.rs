use std::collections::HashMap;

use crate::models::{model::Role, thread::Thread};

pub trait Command {
    fn name(&self) -> &str;
    fn process(&self, thread: &Thread) -> Vec<String>;
    fn command_type(&self) -> &CommandType; 
}

pub struct CommandController {
    commands: HashMap<String, Box<dyn Command + Send + Sync>>,
}

impl CommandController {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }
    pub fn add_command(&mut self, command: impl Command + Send + Sync + 'static) {
        self.commands
            .insert(command.name().to_owned(), Box::new(command));
    }
    pub fn execute(&self, command_name: &str, thread: &Thread) -> Option<Vec<String>> {
        self.commands.get(command_name).map(|cmd| cmd.process(thread))
    }
}

pub enum CommandType {
    ContextChanger,
    UserPrompt,
    Info,
}

pub struct CommandPrompt {
    prompt: String,
    choices: Vec<String>,
}

pub struct TreeCommand {
    command_type: CommandType,
}

impl TreeCommand {
    pub fn new() -> Self {
        Self { command_type: CommandType::Info}
    }
}

impl Command for TreeCommand {
    fn name(&self) -> &str {
        "tree"
    }

    fn command_type(&self) -> &CommandType {
        &self.command_type
    }

    fn process(&self, thread: &Thread) -> Vec<String> {
        let mut result = Vec::new();
        for thread_item in thread.context.iter() {
            match thread_item.role {
                Role::System | Role::User => {
                    if let Some(content) = &thread_item.content {
                        result.push(format!("{}: {}", thread_item.role.to_string(), content));
                    } else {
                        result.push(format!(
                            "{}: {}",
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
}
