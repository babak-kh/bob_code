use crossterm::event::{Event, EventStream, KeyCode};
use futures::StreamExt;
use ratatui::prelude::*;
use std::env;
use std::sync::Arc;
use thiserror::Error;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

use crate::agent::{groq, ollama, openrouter};
use crate::commands::{CommandEffect, DialogAction};
use crate::components::prompt_dialog::{FieldResponse, PromptDialogController, PromptDialogEvent};
use crate::components::response_block;
use crate::controller;
use crate::models::model::ChatMessageResponse;
use crate::service::profile;
use crate::system_prompt::generate_system_prompt;
use crate::ui::{PromptController, ResponseAreaController, StatusLineController};
use crate::{controller::MessageController, prompt::PromptEvent};

#[derive(PartialEq)]
enum FocusedPanel {
    Prompt,
    Response,
}

pub struct App {
    config_base_path: String,
    name: String,
    version: String,
    prompt: PromptController,
    pub(super) controller: MessageController,
    response_area_controller: ResponseAreaController,
    status_line: StatusLineController,
    focused: FocusedPanel,
    /// When `Some`, a floating dialog is active and captures all key events.
    /// The `DialogAction` records what to do when the user submits.
    dialog: Option<(PromptDialogController, DialogAction)>,
    user_profile: profile::UserProfile,
}

impl App {
    pub fn new() -> Self {
        let config_base_path = std::env::var("BOB_CODE_CONFIG_PATH").unwrap_or_else(|_| {
            dirs::config_dir()
                .unwrap()
                .join("bob_code")
                .to_str()
                .unwrap()
                .to_string()
        });

        let mut user_profile = profile::UserProfile::new(config_base_path.clone());

        user_profile
            .fetch(profile::ProfileDefaults {
                model: "gpt-oss-120b".to_string(),
            })
            .expect("Failed to fetch user profile data");

        tracing::debug!("User profile: {:?}", user_profile);

        Self {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            config_base_path,
            user_profile,

            prompt: PromptController::new(),
            controller: MessageController::new(),
            response_area_controller: ResponseAreaController::new(),
            status_line: StatusLineController::new(),
            focused: FocusedPanel::Prompt,
            dialog: None,
        }
    }

    pub async fn run<B: Backend>(mut self, terminal: &mut Terminal<B>) {
        register_models(&mut self.controller).expect("Failed to register models");

        self.controller
            .set_current_model(self.user_profile.get_model())
            .expect("Failed to set model from user profile");

        if let Some(name) = self.controller.current_model_name().map(|s| s.to_string()) {
            self.status_line.set_model_name(name);
        }
        let _thread_id = self.controller.new_thread();
        self.controller.set_system(generate_system_prompt());
        let (gpu_info_channel_tx, mut gpu_info_channel_rx) = mpsc::unbounded_channel::<String>();

        let (resp_tx, mut resp_rx) = broadcast::channel::<ChatMessageResponse>(1000);
        let mut event_stream = EventStream::new();
        let gpu_monitor = crate::service::system_call::NvidiaSmi::new();
        tokio::spawn(gpu_monitor.monitor_gpu(gpu_info_channel_tx));

        terminal.draw(|f| self.ui(f)).unwrap();

        loop {
            select! {
                Some(event) = event_stream.next() => {
                    tracing::info!("Event: {:?}", event);
                    match event {
                        Ok(event) => {
                            if is_quit(&event) {
                                self.user_profile.save().expect("Failed to flush user profile data");
                                break;
                            }

                            // Dialog captures key events only.
                            if let Event::Key(key_event) = &event {
                                if let Some((dialog, action)) = &mut self.dialog {
                                    match dialog.handle_key(*key_event) {
                                        Some(PromptDialogEvent::Submitted(resp)) => {
                                            let action = std::mem::replace(
                                                action,
                                                DialogAction::SelectModel,
                                            );
                                            self.dialog = None;
                                            self.handle_dialog_submit(resp, action);
                                        }
                                        Some(PromptDialogEvent::Cancelled) => {
                                            self.dialog = None;
                                        }
                                        None => {}
                                    }
                                    self.redraw(terminal);
                                    continue;
                                }

                                if key_event.code == KeyCode::Tab {
                                    self.handle_focus_change();
                                    terminal.draw(|f| self.ui(f)).unwrap();
                                    continue;
                                }
                            }

                            match self.focused {
                                FocusedPanel::Response => {
                                    if let Event::Key(key_event) = event {
                                        self.response_area_controller.handle_key_event(key_event);
                                    }
                                }
                                FocusedPanel::Prompt => {
                                    if let Some(prompt_event) = self.prompt.handle_event(&event) {
                                        self.handle_prompt_event(prompt_event, resp_tx.clone()).await;
                                    }
                                }
                            }
                            self.redraw(terminal);
                        }
                        Err(e) => {
                            tracing::error!("Error reading event: {}", e);
                        }
                    };
                }
                Ok(resp) = resp_rx.recv() => {
                    tracing::info!("Received response: {:?}", resp);
                    if let Some(e) = resp.error {
                        self.response_area_controller
                            .add_block(response_block::error_block(e));
                        self.redraw(terminal);
                        continue;
                    }
                    if let Some(tool_calls) = resp.tool_calls {
                        self.controller.set_tool_calls(tool_calls.clone());
                        let tool_json = serde_json::to_string_pretty(&tool_calls)
                            .unwrap_or_else(|_| "Failed to serialize tool call".to_string());
                        self.response_area_controller
                            .add_block(response_block::tool_block(tool_json));
                        let tool_response = MessageController::handle_tool_calls(tool_calls.as_ref()).await;
                        for resp in &tool_response {
                            self.controller.set_tool_call_response(resp.clone());
                            let resp_json = serde_json::to_string_pretty(resp)
                                .unwrap_or_else(|_| "Failed to serialize tool response".to_string());
                            self.response_area_controller
                                .add_block(response_block::tool_block(resp_json));
                        }
                        let (model, thread) = self.controller.prepare_call(self.controller.current_model_name().unwrap().to_string());
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
                    if let Some(thinking) = resp.thinking && !thinking.is_empty() {
                            self.response_area_controller
                                .add_block(response_block::thinking_block(thinking));
                    }
                    if let Some(content) = resp.content && !content.is_empty() {
                            self.response_area_controller
                                .add_block(response_block::assistant_block(content));
                    }
                    self.redraw(terminal);
                }
                Some(gpu_info) = gpu_info_channel_rx.recv() => {
                    self.status_line.set_gpu_info(gpu_info);
                            self.redraw(terminal);
                }
            }
        }
    }

    fn ui(&mut self, f: &mut ratatui::Frame) {
        let prompt_h = self.prompt.desired_height();
        let chunks = Layout::default()
            .margin(0)
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Fill(1),          // response — takes remaining space
                Constraint::Length(prompt_h), // prompt — content-sized, max 5 lines
                Constraint::Length(1),        // status line
            ])
            .split(f.area());

        self.response_area_controller.render(f, chunks[0]);
        self.prompt.render(f, chunks[1]);
        self.status_line.render(f, chunks[2]);

        // Render the dialog as the topmost layer so it overlays everything.
        if let Some((dialog, _)) = &self.dialog {
            dialog.render(f);
        }
    }

    /// Apply the response from a submitted dialog based on what opened it.
    fn handle_dialog_submit(
        &mut self,
        resp: crate::components::prompt_dialog::PromptDialogResponse,
        action: DialogAction,
    ) {
        match action {
            DialogAction::SelectModel => {
                if let Some(FieldResponse::SingleChoice { value, .. }) = resp.get("model") {
                    let name = value.clone();
                    self.controller.set_current_model(name.clone());
                    self.status_line.set_model_name(name.clone());
                    self.user_profile.set_model(name.clone());
                    self.response_area_controller
                        .add_block(response_block::command_block(format!("Switched to {name}")));
                }
            }
        }
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
            PromptEvent::Command { name, args } => {
                if let Some(cmd) = Self::parse_command(&name, &args) {
                    let cmd_effect = self.handle_command(cmd, &args);
                    match cmd_effect {
                        CommandEffect::None => {}
                        CommandEffect::ResponseArea(text) => {
                            self.response_area_controller
                                .add_block(response_block::command_block(text));
                        }
                        CommandEffect::OpenDialog { schema, action } => {
                            self.dialog = Some((PromptDialogController::new(schema), action));
                        }
                    }
                }
            }
            PromptEvent::Submitted(text) => {
                // Show the user message immediately in the response area
                self.response_area_controller
                    .add_block(response_block::user_block(text.clone()));

                self.controller.set_prompt(text);
                let (model, thread) = self
                    .controller
                    .prepare_call(self.controller.current_model_name().unwrap().to_string());
                let model_clone = model.unwrap().clone();
                let resp_tx_clone = resp_tx.clone();
                tokio::spawn(async move {
                    model_clone.generate(&thread, resp_tx_clone).await;
                });
            }
        }
    }
    fn redraw(&mut self, terminal: &mut Terminal<impl Backend>) {
        terminal.draw(|f| self.ui(f)).unwrap();
    }
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Model registration failed: {0}")]
    ModelRegisterationError(String),
}

fn register_models(controller: &mut controller::MessageController) -> Result<(), AppError> {
    let gemma4_model = ollama::gemma4::GEMMA4Model;
    controller.register_model(Arc::new(gemma4_model));
    let grok_model = groq::GroqBase::new(groq::GroqModel::GptOss120B, "".to_string());
    controller.register_model(Arc::new(grok_model));
    let openrouter_model =
        openrouter::OpenRouterBase::new(openrouter::OpenRouterModel::DeepSeekR1, "".to_string());
    controller.register_model(Arc::new(openrouter_model));
    Ok(())
}

fn is_quit(event: &Event) -> bool {
    if let Event::Key(key_event) = event {
        return key_event.code == KeyCode::Char('d')
            && key_event
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL);
    }
    false
}
