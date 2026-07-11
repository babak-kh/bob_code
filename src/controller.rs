use std::{collections::HashMap, sync::Arc};
use tokio::task::JoinHandle;

use crate::{
    models::{
        model::LLMModel,
        thread::{ContextItem, Thread},
        tool::ToolCallResponse,
    },
    tool::{ToolCallRequest, execute_tool},
};

/// Conversation state repository. Does NOT own thread selection — the
/// caller passes `thread_id` into every method that mutates or reads
/// a specific thread.
///
/// Threads are stored in a `HashMap<String, Thread>` keyed by a stable
/// id so that removals do not invalidate other thread references.
/// The `running_handlers` map associates each thread id with its active
/// `generate` task so that cancellation targets the correct task.
#[derive(Default)]
pub struct MessageController {
    pub threads: HashMap<usize, Thread>,
    pub models: HashMap<String, Arc<dyn LLMModel + Send + Sync>>,
    pub current_model: Option<String>,
    pub running_handlers: HashMap<usize, JoinHandle<()>>,
    next_thread_id: usize,
}

impl MessageController {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Thread lifecycle ────────────────────────────────────────────────

    /// Allocate a new thread, returning its stable id.
    /// The caller is responsible for remembering which thread is active.
    pub fn new_thread(&mut self) -> usize {
        let thread_id = self.next_thread_id;
        self.next_thread_id += 1;
        self.threads
            .insert(thread_id, Thread::new(thread_id.to_string()));
        thread_id
    }

    /// Remove a thread and cancel its running handler (if any).
    /// Returns `true` if a thread was actually removed.
    pub fn remove_thread(&mut self, thread_id: usize) -> bool {
        self.cancel_running(thread_id);
        self.threads.remove(&thread_id).is_some()
    }

    // ── Running handlers ────────────────────────────────────────────────

    pub fn add_running(&mut self, thread_id: usize, req: JoinHandle<()>) {
        self.running_handlers.insert(thread_id, req);
    }

    pub fn cancel_running(&mut self, thread_id: usize) -> bool {
        if let Some(handle) = self.running_handlers.remove(&thread_id) {
            handle.abort();
            true
        } else {
            false
        }
    }

    // ── Model registry ──────────────────────────────────────────────────

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

    pub fn get_model_by_name(&self, model_name: &str) -> Option<&Arc<dyn LLMModel + Send + Sync>> {
        self.models.get(model_name)
    }

    pub fn current_model_name(&self) -> Option<&str> {
        self.current_model.as_deref()
    }

    // ── Thread access ───────────────────────────────────────────────────

    pub fn get_thread(&self, thread_id: usize) -> Option<&Thread> {
        self.threads.get(&thread_id)
    }

    // ── Conversation mutations (all take explicit thread_id) ────────────

    pub fn set_system(&mut self, thread_id: usize, system_message: String) {
        if let Some(thread) = self.threads.get_mut(&thread_id) {
            thread.add_context(ContextItem::system(system_message));
        }
    }

    pub fn set_prompt(&mut self, thread_id: usize, prompt: String) {
        if let Some(thread) = self.threads.get_mut(&thread_id) {
            thread.add_context(ContextItem::user(prompt));
        }
    }

    pub fn set_response(&mut self, thread_id: usize, response: String) {
        if let Some(thread) = self.threads.get_mut(&thread_id) {
            thread.add_context(ContextItem::assistant(response));
        }
    }

    pub fn set_tool_calls(&mut self, thread_id: usize, tool_calls: Vec<ToolCallRequest>) {
        if let Some(thread) = self.threads.get_mut(&thread_id) {
            thread.add_context(ContextItem::tool_request(tool_calls));
        }
    }

    pub fn set_tool_call_response(&mut self, thread_id: usize, tool_response: ToolCallResponse) {
        if let Some(thread) = self.threads.get_mut(&thread_id) {
            thread.add_context(ContextItem::tool_response(tool_response));
        }
    }

    /// Snapshot the requested thread and resolve the model for a generate call.
    pub fn prepare_call(
        &self,
        thread_id: usize,
        model_name: &str,
    ) -> (Option<&Arc<dyn LLMModel + Send + Sync>>, Option<Thread>) {
        let thread = self.threads.get(&thread_id).cloned();
        let model = self.get_model_by_name(model_name);
        (model, thread)
    }

    // ── Tool execution (static, no state needed) ────────────────────────

    pub async fn handle_tool_calls(tool_calls: &[ToolCallRequest]) -> Vec<ToolCallResponse> {
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
}
