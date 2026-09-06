//! DeskPet — application entry point.

pub mod core;
mod platform;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, WebviewWindow,
};

use crate::core::{config::Config, hitmask::MaskSet, webhook};
use crate::platform::{PetWindow, StackLevel};

/// Which frame the renderer is currently showing. Written from the frontend
/// on every frame change, read by the hit-test loop.
#[derive(Default)]
pub struct FrameState {
    pub frame_id: String,
}

pub struct AppState {
    pub masks: Mutex<MaskSet>,
    pub frame: Mutex<FrameState>,
    pub config: Mutex<Config>,
}

/// Reported by the frontend whenever the displayed frame changes.
#[tauri::command]
fn set_current_frame(state: tauri::State<AppState>, frame_id: String) {
    state.frame.lock().unwrap().frame_id = frame_id;
}

#[tauri::command]
fn get_config(state: tauri::State<AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

/// Poll the cursor and toggle click-through.
///
/// Polling rather than reacting to mousemove is deliberate: a click-through
/// window receives no mouse events, so there is nothing to react to. 30 Hz is
/// below the threshold where the toggle feels laggy and cheap enough to be
/// invisible in Activity Monitor.
fn spawn_hit_test_loop(
    window: WebviewWindow,
    plat: Arc<Box<dyn PetWindow>>,
    state: Arc<AppState>,
) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(33));

        let Ok(cursor) = window.cursor_position() else { continue };
        let Ok(origin) = window.outer_position() else { continue };
        let scale = plat.scale_factor();

        let lx = (cursor.x - origin.x as f64) / scale;
        let ly = (cursor.y - origin.y as f64) / scale;

        let frame_id = state.frame.lock().unwrap().frame_id.clone();
        let over_pet = state.masks.lock().unwrap().hit(&frame_id, lx, ly);

        let _ = plat.set_click_through(!over_pet);
    });
}

/// Watch for fullscreen apps and focus mode at 1 Hz.
fn spawn_environment_loop(
    window: WebviewWindow,
    plat: Arc<Box<dyn PetWindow>>,
    state: Arc<AppState>,
) {
    std::thread::spawn(move || {
        let mut hidden = false;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let cfg = state.config.lock().unwrap().clone();

            if cfg.hide_on_fullscreen {
                let fullscreen = plat.is_fullscreen_app_active();
                if fullscreen != hidden {
                    hidden = fullscreen;
                    let _ = if fullscreen {
                        plat.set_stack_level(StackLevel::Normal).ok();
                        window.hide()
                    } else {
                        plat.set_stack_level(StackLevel::Floating).ok();
                        window.show()
                    };
                }
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![set_current_frame, get_config])
        .setup(|app| {
            let handle = app.handle().clone();

            let mut cfg = Config::load(&handle);
            if cfg.webhook_token.is_empty() {
                cfg.webhook_token = webhook::generate_token();
                cfg.save(&handle).ok();
            }

            let state = Arc::new(AppState {
                masks: Mutex::new(MaskSet::default()),
                frame: Mutex::new(FrameState::default()),
                config: Mutex::new(cfg.clone()),
            });
            app.manage(AppState {
                masks: Mutex::new(MaskSet::default()),
                frame: Mutex::new(FrameState::default()),
                config: Mutex::new(cfg.clone()),
            });

            let window = app
                .get_webview_window("pet")
                .expect("window 'pet' missing from tauri.conf.json");

            // Everything OS-specific happens behind this call.
            let plat = Arc::new(platform::attach(&window)?);
            plat.configure()?;
            plat.set_stack_level(StackLevel::Floating)?;
            window.show()?;

            spawn_hit_test_loop(window.clone(), plat.clone(), state.clone());
            spawn_environment_loop(window.clone(), plat.clone(), state.clone());

            let quit = MenuItem::with_id(app, "quit", "Quit DeskPet", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings, &quit])?;

            TrayIconBuilder::new()
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    // The settings window is created on demand and destroyed on
                    // close, so its webview does not sit in memory all day.
                    "settings" => open_settings(app),
                    _ => {}
                })
                .build(app)?;

            let hook_app = handle.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) =
                    webhook::serve(hook_app, cfg.webhook_token, cfg.webhook_port).await
                {
                    tracing::error!("webhook server stopped: {e}");
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running DeskPet");
}

fn open_settings(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.set_focus();
        return;
    }
    let _ = tauri::WebviewWindowBuilder::new(
        app,
        "settings",
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title("DeskPet Settings")
    .inner_size(720.0, 560.0)
    .resizable(true)
    .build();
}
