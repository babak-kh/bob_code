use crossterm::event::{Event, EventStream, KeyCode};
use futures::StreamExt;
use ratatui::{layout::Flex, prelude::*};
use std::env;
use std::sync::Arc;
use thiserror::Error;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

use crate::agent::{groq, ollama};
use crate::components::prompt_dialog::{PromptDialogController, PromptDialogEvent, PromptSchema};
use crate::controller;
use crate::models::display::ResponseAreaInput;
use crate::models::model::ChatMessageResponse;
use crate::system_prompt::generate_system_prompt;
use crate::ui::{PromptController, ResponseAreaController, StatusLineController};
use crate::{
    controller::MessageController,
    prompt::PromptEvent,
    service::commands::{CommandController, CommandType, TreeCommand},
};

#[derive(PartialEq)]
enum FocusedPanel {
    Prompt,
    Response,
}

pub struct App {
    name: String,
    version: String,
    prompt: PromptController,
    controller: MessageController,
    response_area_controller: ResponseAreaController,
    status_line: StatusLineController,
    command_controller: CommandController,
    focused: FocusedPanel,
    /// When `Some`, a floating dialog is active and captures all key events.
    dialog: Option<PromptDialogController>,
}

impl App {
    pub fn new() -> Self {
        Self {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            prompt: PromptController::new(),
            controller: MessageController::new(),
            response_area_controller: ResponseAreaController::new(),
            status_line: StatusLineController::new(),
            focused: FocusedPanel::Prompt,
            command_controller: CommandController::new(),
            dialog: None,
        }
    }

    pub async fn run<B: Backend>(mut self, terminal: &mut Terminal<B>) {
        register_models(&mut self.controller).expect("Failed to register models");
        register_commands(&mut self.command_controller);
        let _thread_id = self.controller.new_thread();
        self.controller.set_system(generate_system_prompt());
        println!("thread id: {}", _thread_id);
        let (gpu_info_channel_tx, mut gpu_info_channel_rx) = mpsc::unbounded_channel::<String>();

        let (resp_tx, mut resp_rx) = broadcast::channel::<ChatMessageResponse>(1000);
        let mut event_stream = EventStream::new();
        let gpu_monitor = crate::service::system_call::NvidiaSmi::new();
        tokio::spawn(gpu_monitor.monitor_gpu(gpu_info_channel_tx));

        terminal.draw(|f| self.ui(f)).unwrap();

        loop {
            select! {
                Some(event) = event_stream.next() => {
                    match event {
                        Ok(Event::Key(key_event)) => {
                            // Global quit: Ctrl+C
                            if is_quit(&Event::Key(key_event)) {
                                break;
                            }

                            // If a dialog is open, route all keys to it first.
                            if let Some(dialog) = &mut self.dialog {
                                match dialog.handle_key(key_event) {
                                    Some(PromptDialogEvent::Submitted(resp)) => {
                                        let summary = serde_json::to_string_pretty(&resp)
                                            .unwrap_or_else(|_| "(serialization error)".into());
                                        tracing::info!("Dialog submitted: {summary}");
                                        // TODO: forward `resp` to the AI tool-call machinery
                                        self.dialog = None;
                                    }
                                    Some(PromptDialogEvent::Cancelled) => {
                                        self.dialog = None;
                                    }
                                    None => {}
                                }
                                terminal.draw(|f| self.ui(f)).unwrap();
                                continue;
                            }

                            // Tab: toggle focus between Prompt and Response
                            if key_event.code == KeyCode::Tab {
                                self.handle_focus_change();
                                terminal.draw(|f| self.ui(f)).unwrap();
                                continue;
                            }

                            match self.focused {
                                FocusedPanel::Response => {
                                    self.response_area_controller.handle_key_event(key_event);
                                }
                                FocusedPanel::Prompt => {
                                    let prompt_event = self.prompt.handle_event(&event.unwrap());
                                    if let Some(prompt_event) = prompt_event {
                                        self.handle_prompt_event(prompt_event, resp_tx.clone()).await;
                                    }
                                }
                            }
                            terminal.draw(|f| self.ui(f)).unwrap();
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::error!("Error reading event: {}", e);
                        }
                    };
                }
                Ok(resp) = resp_rx.recv() => {
                    tracing::info!("Received response: {:?}", resp);
                    if let Some(tool_calls) = resp.tool_calls {
                        self.controller.set_tool_calls(tool_calls.clone());
                        ResponseAreaInput::tool_call(tool_calls.clone());
                        let tool_response = MessageController::handle_tool_calls(tool_calls.as_ref()).await;
                        for resp in &tool_response {
                            self.controller.set_tool_call_response(resp.clone());
                            self.response_area_controller.add_to_payload(
                                ResponseAreaInput::tool_response(resp.clone())
                            ).await;
                        }
                        let (model, thread) = self.controller.prepare_call("openai/gpt-oss-120b".to_string());
                        let model_clone = model.unwrap().clone();
                        let resp_tx_clone = resp_tx.clone();
                        tokio::spawn(async move {
                            model_clone.generate(&thread, resp_tx_clone).await;
                        });
                    }
                    if resp.done {
                        self.controller.set_response(resp.content.clone().unwrap_or_default());
                        continue;
                    }
                    if let Some(thinking) = resp.thinking {
                        if !thinking.is_empty() {
                            self.response_area_controller.add_to_payload(
                                ResponseAreaInput::assistant_thinking(thinking)
                            ).await;
                        }
                    }
                    if let Some(content) = resp.content {
                        if !content.is_empty() {
                            self.response_area_controller.add_to_payload(
                                ResponseAreaInput::assistant_content(content)
                            ).await;
                        }
                    }
                    terminal.draw(|f| self.ui(f)).unwrap();
                }
                Some(gpu_info) = gpu_info_channel_rx.recv() => {
                    self.status_line.set_gpu_info(gpu_info);
                    terminal.draw(|f| self.ui(f)).unwrap();
                }
            }
        }
    }

    fn ui(&mut self, f: &mut ratatui::Frame) {
        let chunks = Layout::default()
            .flex(Flex::Center)
            .margin(0)
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Percentage(80), // chat / response
                Constraint::Fill(1),        // prompt (remaining height)
                Constraint::Length(1),      // status line (borderless, 1 row)
            ])
            .split(f.area());

        self.response_area_controller.render(f, chunks[0]);
        self.prompt.render(f, chunks[1]);
        self.status_line.render(f, chunks[2]);

        // Render the dialog as the topmost layer so it overlays everything.
        if let Some(dialog) = &self.dialog {
            dialog.render(f);
        }
    }

    /// Open a prompt dialog. While open, all key events are captured by the
    /// dialog and normal prompt / response navigation is suspended.
    #[allow(dead_code)]
    pub fn open_dialog(&mut self, schema: PromptSchema) {
        self.dialog = Some(PromptDialogController::new(schema));
    }

    fn handle_focus_change(&mut self) {
        match self.focused {
            FocusedPanel::Prompt => {
                self.focused = FocusedPanel::Response;
                self.prompt.set_focus(false);
                self.response_area_controller.set_focus(true);
            }
            FocusedPanel::Response => {
                self.focused = FocusedPanel::Prompt;
                self.prompt.set_focus(true);
                self.response_area_controller.set_focus(false);
            }
        }
    }
    async fn handle_prompt_event(
        &mut self,
        prompt_event: PromptEvent,
        resp_tx: broadcast::Sender<ChatMessageResponse>,
    ) {
        match prompt_event {
            PromptEvent::Command(name) => {
                if let Some(thread) = self.controller.get_current_thread() {
                    let command_response = self.command_controller.execute(&name, thread);
                    if let Some(response) = command_response {
                        self.response_area_controller
                            .add_to_payload(ResponseAreaInput::info_command_output(
                                response.join("\n"),
                            ))
                            .await;
                    }
                }
            }
            PromptEvent::Submitted(text) => {
                // Show the user message immediately in the response area
                self.response_area_controller
                    .add_to_payload(ResponseAreaInput::user(text.clone()))
                    .await;

                self.controller.set_prompt(text);
                let (model, thread) = self
                    .controller
                    .prepare_call("openai/gpt-oss-120b".to_string());
                let model_clone = model.unwrap().clone();
                let resp_tx_clone = resp_tx.clone();
                tokio::spawn(async move {
                    model_clone.generate(&thread, resp_tx_clone).await;
                });
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Model registration failed: {0}")]
    ModelRegisterationError(String),
}

fn register_commands(controller: &mut CommandController) {
    controller.add_command(TreeCommand::new())
}

fn register_models(controller: &mut controller::MessageController) -> Result<(), AppError> {
    let gemma4_model = ollama::gemma4::GEMMA4Model;
    controller.register_model(Arc::new(gemma4_model));
    let grok_model = groq::GroqBase::new(
        groq::GroqModel::GptOss120B,
        "".to_string(),
    );
    controller.register_model(Arc::new(grok_model));
    Ok(())
}

fn is_command(event: &Event) -> bool {
    if let Event::Key(key_event) = event {
        return key_event
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
            && matches!(key_event.code, KeyCode::Char('c') | KeyCode::Char('i'));
    }
    false
}

fn is_quit(event: &Event) -> bool {
    if let Event::Key(key_event) = event {
        return key_event.code == KeyCode::Char('c')
            && key_event
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL);
    }
    false
}
