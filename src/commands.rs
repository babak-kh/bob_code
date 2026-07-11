use crate::app::App;
use crate::components::prompt_dialog::{FieldSchema, PromptSchema};
use crate::service::commands::render_tree;

// ── Command names ──────────────────────────────────────────────────────────────

pub enum Command {
    Model,
    Tree,
    NewThread,
}

// ── Effects ────────────────────────────────────────────────────────────────────

/// What should happen after a command is executed.
pub enum CommandEffect {
    /// Nothing to do.
    None,
    /// Display text in the response area.
    ResponseArea(String),
    /// Open a dialog. `action` tells App what to do when the user submits.
    OpenDialog {
        schema: PromptSchema,
        action: DialogAction,
    },
    ClearThread,
}

/// What App should do when a dialog opened by a command is submitted.
pub enum DialogAction {
    SelectModel,
}

// ── Parsing & handling ─────────────────────────────────────────────────────────

impl App {
    /// Map a slash-command name (and its args) to a typed `Command`.
    /// Args are received here so the signature never needs to change as
    /// commands become more complex.
    pub fn parse_command(name: &str, _args: &[String]) -> Option<Command> {
        match name {
            "models" => Some(Command::Model),
            "tree" => Some(Command::Tree),
            "new" => Some(Command::NewThread),
            _ => None,
        }
    }

    /// Execute a parsed command against `self` and return an effect.
    /// All mutations happen in App — this method only reads state and
    /// returns an intent.
    pub fn handle_command(&mut self, command: Command, _args: &[String]) -> CommandEffect {
        match command {
            Command::Model => {
                let model_names = self.controller.get_model_names();
                let schema = PromptSchema::new("Select a model")
                    .with_field(FieldSchema::single_choice("model", "Model", model_names));
                CommandEffect::OpenDialog {
                    schema,
                    action: DialogAction::SelectModel,
                }
            }
            Command::Tree => {
                let tid = self.current_thread_id;
                match self.controller.get_thread(tid) {
                    Some(thread) => CommandEffect::ResponseArea(render_tree(thread).join("\n")),
                    None => CommandEffect::None,
                }
            }
            Command::NewThread => {
                // Cancel the old handler and remove the old thread.
                self.controller.remove_thread(self.current_thread_id);
                CommandEffect::ClearThread
            }
        }
    }
}
