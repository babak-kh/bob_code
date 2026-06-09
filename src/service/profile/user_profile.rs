use serde::{Deserialize, Serialize};
use serde_json::{self, error::Category};
use std::{io::ErrorKind, path::Path};

const SETTINGS_SUB_PATH: &str = "settings";
const SETTINGS_FILE_NAME: &str = "settings.json";

const SESSIONS_SUB_PATH: &str = "sessions";
const TEMP_SUB_PATH: &str = "temp";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    model: String,
}
impl Settings {
    pub fn new(model: String) -> Self {
        Self { model }
    }
    pub fn set_model(&mut self, model: String) {
        self.model = model;
    }
}

pub struct ProfileDefaults {
    pub model: String,
}

#[derive(Debug, Clone, Default)]
pub struct UserProfile {
    path: String,
    settings: Settings,
}

impl UserProfile {
    pub fn new(path: String) -> Self {
        Self {
            path,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn initialize(&mut self) -> Result<(), String> {
        if !Path::new(&self.path).exists() {
            std::fs::create_dir_all(&self.path).map_err(|e| e.to_string())?;
        }
        let settings_path = Path::new(&self.path).join(SETTINGS_SUB_PATH);
        let sessions_path = Path::new(&self.path).join(SESSIONS_SUB_PATH);
        let temp_path = Path::new(&self.path).join(TEMP_SUB_PATH);
        if !settings_path.exists() {
            std::fs::create_dir_all(&settings_path).map_err(|e| e.to_string())?;
        }
        if !sessions_path.exists() {
            std::fs::create_dir_all(&sessions_path).map_err(|e| e.to_string())?;
        }
        if !temp_path.exists() {
            std::fs::create_dir_all(&temp_path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn set_model(&mut self, model: String) {
        self.settings.set_model(model);
    }

    pub fn get_model(&self) -> String {
        self.settings.model.clone()
    }

    pub fn fetch(&mut self, defaults: ProfileDefaults) -> Result<(), String> {
        // read settings, sessions, temp data
        match serde_json::from_str::<Settings>(
            match &std::fs::read_to_string(
                Path::new(&self.path)
                    .join(SETTINGS_SUB_PATH)
                    .join(SETTINGS_FILE_NAME),
            ) {
                Ok(content) => content,
                Err(e) => match e.kind() {
                    ErrorKind::NotFound => return Ok(()),
                    _ => return Err(e.to_string()),
                },
            },
        ) {
            Ok(settings) => self.settings = settings,
            Err(e) => match e.classify() {
                Category::Syntax | Category::Data => {
                    // If the settings file is corrupted, we can choose to ignore it and use defaults
                    tracing::warn!(
                        "Warning: Settings file is corrupted. Using default settings. Error: {}",
                        e
                    );
                    self.settings = Settings::new(defaults.model);
                }
                _ => return Err(e.to_string()),
            },
        }

        Ok(())
    }

    pub fn save(&self) -> Result<(), String> {
        // save settings, sessions, temp data
        std::fs::write(
            self.path.clone() + "/" + SETTINGS_SUB_PATH + "/" + SETTINGS_FILE_NAME,
            serde_json::to_string(&self.settings).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
