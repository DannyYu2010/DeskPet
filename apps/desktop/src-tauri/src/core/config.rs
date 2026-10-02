//! User configuration, persisted as TOML in the OS config directory.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Directory name of the active pack under `packs/`.
    pub active_pack: String,
    pub webhook_port: u16,
    pub webhook_token: String,
    /// Honour the OS focus / do-not-disturb state.
    pub respect_dnd: bool,
    /// Hide entirely when a fullscreen app is frontmost.
    pub hide_on_fullscreen: bool,
    /// Suppress reactions while the user is actively typing. Measured from
    /// our own window's idle time, not by reading keystrokes.
    pub quiet_while_typing: bool,
    pub scale: f32,
    /// Frames per second when the pet window is not focused.
    pub unfocused_fps: u8,
    /// Seconds without interaction before the pet enters its sleep state.
    pub sleep_after_seconds: u32,
    /// Launch DeskPet automatically after the user signs in. Off by default.
    pub start_at_login: bool,
    /// Last user-chosen window origin in physical screen pixels.
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            active_pack: "default".into(),
            webhook_port: crate::core::webhook::DEFAULT_PORT,
            webhook_token: String::new(),
            respect_dnd: true,
            hide_on_fullscreen: true,
            quiet_while_typing: true,
            scale: 1.0,
            unfocused_fps: 10,
            sleep_after_seconds: 90,
            start_at_login: false,
            window_x: None,
            window_y: None,
        }
    }
}

impl Config {
    pub fn path(app: &tauri::AppHandle) -> anyhow::Result<std::path::PathBuf> {
        use tauri::Manager;
        let dir = app.path().app_config_dir()?;
        std::fs::create_dir_all(&dir)?;
        Ok(dir.join("config.toml"))
    }

    pub fn load(app: &tauri::AppHandle) -> Self {
        Self::path(app)
            .ok()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, app: &tauri::AppHandle) -> anyhow::Result<()> {
        std::fs::write(Self::path(app)?, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}
