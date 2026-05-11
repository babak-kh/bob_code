use crate::tool;
use std::env;

pub fn generate_system_prompt() -> String {
    let mut base = format!(
        "You are an intelligent software developer and architect. \
                respond to the user messages with helpful answers, and use tools when necessary. \
                current working directory is {}\n",
        env::current_dir().unwrap().to_string_lossy()
    );

    base.push_str("current data and time is: ");
    base.push_str(&chrono::Local::now().to_string());
    base.push_str("\n\n");

    let tools = tool::tools_catalog()
        .into_iter()
        .map(|tool| format!("Tool: {}\nDescription: {}", tool.name, tool.description))
        .collect::<Vec<String>>()
        .join("\n\n");

    if !tools.is_empty() {
        base.push_str("Available tools:\n\n");
        base.push_str(&tools);
    }

    //if let Some(agents_info) = check_agents_file() {
    //    base.push_str("\n\nAgents information:\n\n");
    //    base.push_str(&agents_info);
    //}

    String::from(base)
}

fn check_agents_file() -> Option<String> {
    let path = env::current_dir().unwrap().join("AGENTS.md");
    if path.exists() {
        std::fs::read_to_string(path).ok()
    } else {
        None
    }
}
