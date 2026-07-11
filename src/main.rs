mod agent;
mod app;
mod commands;
mod components;
mod controller;
mod models;
mod prompt;
mod service;
mod system_prompt;
mod tool;
mod ui;

use color_eyre::eyre::Result;
use std::sync::LazyLock;
use std::io;
use std::path::PathBuf;
use tracing_error::ErrorLayer;
use tracing_subscriber::{self, EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crossterm::{
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    //    setup_terminal()?;
    //    env::set_var("RUST_BACKTRACE", "1");
    initialize_logging().unwrap();
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        )
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let app = app::App::new();
    app.run(&mut terminal).await;
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste,
        PopKeyboardEnhancementFlags
    )?;
    terminal.show_cursor()?;
    Ok(())
}

pub static PROJECT_NAME: LazyLock<String> =
    LazyLock::new(|| env!("CARGO_CRATE_NAME").to_uppercase().to_string());
pub static DATA_FOLDER: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    std::env::var(format!("{}_DATA", &*PROJECT_NAME))
        .ok()
        .map(PathBuf::from)
});
pub static LOG_ENV: LazyLock<String> =
    LazyLock::new(|| format!("{}_LOGLEVEL", &*PROJECT_NAME));
pub static LOG_FILE: LazyLock<String> =
    LazyLock::new(|| format!("{}.log", env!("CARGO_PKG_NAME")));

pub fn get_data_dir() -> PathBuf {
    PathBuf::from(".").join(".data")
}

pub fn initialize_logging() -> Result<()> {
    let directory = get_data_dir();
    println!("Directory: {:?}", directory);
    std::fs::create_dir_all(directory.clone())?;
    let log_path = directory.join(&*LOG_FILE);
    println!("log_path: {:?}", log_path);
    let log_file = std::fs::File::create(log_path)?;
    println!("log_file: {:?}", log_file);
    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(false);
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("babak_code=debug"));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_subscriber)
        .with(ErrorLayer::default())
        .init();
    Ok(())
}

/// Similar to the `std::dbg!` macro, but generates `tracing` events rather
/// than printing to stdout.
///
/// By default, the verbosity level for the generated events is `DEBUG`, but
/// this can be customized.
#[macro_export]
macro_rules! trace_dbg {
    (target: $target:expr, level: $level:expr, $ex:expr) => {{
        let _ = match $ex {
            value => {
                tracing::event!(target: $target, $level, ?value, stringify!($ex));
                value
            }
        };
    }};
    (level: $level:expr, $ex:expr) => {
        trace_dbg!(target: module_path!(), level: $level, $ex)
    };
    (target: $target:expr, $ex:expr) => {
        trace_dbg!(target: $target, level: tracing::Level::DEBUG, $ex)
    };
    ($ex:expr) => {
        trace_dbg!(level: tracing::Level::DEBUG, $ex)
    };
}
