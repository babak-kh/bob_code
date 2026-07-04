use std::{collections::HashMap, sync::Arc};

use crate::{
    models::{
        model::LLMModel,
        thread::{ContextItem, Thread},
        tool::ToolCallResponse,
    },
    tool::{ToolCallRequest, execute_tool},
};

pub struct MessageController {
    pub threads: Vec<Thread>,
    pub current_thread_id: usize,
    pub models: HashMap<String, Arc<dyn LLMModel + Send + Sync>>,
    pub current_model: Option<String>,
}

impl MessageController {
    pub fn new() -> Self {
        Self {
            threads: Vec::new(),
            current_thread_id: 0,
            models: HashMap::new(),
            current_model: None,
        }
    }

    pub fn new_thread(&mut self) -> usize {
        let thread_id = self.threads.len();
        self.threads.push(Thread::new(thread_id.to_string()));
        self.current_thread_id = thread_id;
        thread_id
    }

    pub fn set_system(&mut self, system_message: String) {
        if let Some(thread) = self.threads.get_mut(self.current_thread_id) {
            thread.add_context(ContextItem::system(system_message));
        }
    }

    pub fn register_model(&mut self, model: Arc<dyn LLMModel + Send + Sync>) {
        let model_name = model.name().to_string();
        self.models.insert(model.name().to_string(), model);
        if self.current_model.is_none() {
            self.current_model = Some(model_name);
        }
    }

    pub fn set_current_model(&mut self, model_name: String) -> Result<(), String> {
        tracing::info!("Setting current model to: {}", model_name);
        if self.models.contains_key(&model_name) {
            self.current_model = Some(model_name);
            Ok(())
        } else {
            Err(format!("Model '{}' not found", model_name))
        }
    }

    pub fn get_model_names(&self) -> Vec<String> {
        self.models.keys().cloned().collect()
    }

    #[allow(dead_code)]
    pub fn get_current_model(&self) -> Option<&Arc<dyn LLMModel + Send + Sync>> {
        if let Some(model_name) = &self.current_model {
            return self.models.get(model_name);
        }
        None
    }

    pub fn get_model_by_name(&self, model_name: &str) -> Option<&Arc<dyn LLMModel + Send + Sync>> {
        self.models.get(model_name)
    }

    pub fn current_model_name(&self) -> Option<&str> {
        self.current_model.as_deref()
    }

    pub fn prepare_call(
        &mut self,
        model_name: String,
    ) -> (Option<&Arc<dyn LLMModel + Send + Sync>>, Thread) {
        let thread = self
            .threads
            .get_mut(self.current_thread_id)
            .unwrap()
            .clone();
        let model = self.get_model_by_name(model_name.as_str());
        (model, thread.clone())
    }

    pub fn set_response(&mut self, response: String) {
        if let Some(thread) = self.threads.get_mut(self.current_thread_id) {
            thread.add_context(ContextItem::assistant(response));
        }
    }

    pub fn set_prompt(&mut self, prompt: String) {
        if let Some(thread) = self.threads.get_mut(self.current_thread_id) {
            thread.add_context(ContextItem::user(prompt));
        }
    }

    pub fn get_current_thread(&self) -> Option<&Thread> {
        self.threads.get(self.current_thread_id)
    }

    pub fn set_tool_calls(&mut self, tool_calls: Vec<ToolCallRequest>) {
        if let Some(thread) = self.threads.get_mut(self.current_thread_id) {
            thread.add_context(ContextItem::tool_request(tool_calls));
        }
    }

    pub async fn handle_tool_calls(tool_calls: &Vec<ToolCallRequest>) -> Vec<ToolCallResponse> {
        let mut responses = Vec::new();
        for call in tool_calls {
            tracing::info!("Handling tool call: {:?}", call);
            let result = execute_tool(call).await;
            let response = ToolCallResponse {
                id: call.id.clone(),
                result: result.text,
                structured: result.structured,
            };
            responses.push(response);
        }
        responses
    }
    pub fn set_tool_call_response(&mut self, tool_response: ToolCallResponse) {
        if let Some(thread) = self.threads.get_mut(self.current_thread_id) {
            thread.add_context(ContextItem::tool_response(tool_response));
        }
    }
}
