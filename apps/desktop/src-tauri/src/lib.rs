//! DeskPet — application entry point.

pub mod core;
mod platform;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, WebviewWindow,
};

use anyhow::Context;
// `ImageDecoder` is needed in scope for `decoder.orientation()`.
use image::ImageDecoder;

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
fn set_current_frame(state: tauri::State<Arc<AppState>>, frame_id: String) {
    state.frame.lock().unwrap().frame_id = frame_id;
}

/// Pipe webview errors into the same stderr stream as everything else.
///
/// Without this a frontend exception is invisible: the pet window is
/// transparent and borderless, so "the script threw" and "the window is not
/// there" look identical from the outside.
#[tauri::command]
fn frontend_log(level: String, message: String) {
    eprintln!("[deskpet/web] {level}: {message}");
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub pack_id: String,
    pub backend: String,
    /// First frame as a `data:` URL, so the settings window can show the
    /// cutout without reading the pack back off disk.
    pub preview: String,
    pub frames: usize,
}

/// One thing the user dropped in: a photo, or the frames of a short clip.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIn {
    /// Which state this becomes: "idle", "sleep", "poke", "notify", ...
    pub state: String,
    /// Base64. One entry for a photo (the original file, EXIF and all), or one
    /// per frame for a clip (PNGs the webview decoded).
    pub frames: Vec<String>,
    /// Quarter turns the user asked for, on top of EXIF. Clockwise.
    pub rotate_degrees: i32,
    /// True when `frames` came out of a video, which changes how they are
    /// decoded (no EXIF) and how the state is timed.
    pub is_clip: bool,
    /// Optional custom-action display name, e.g. “奔跑”.
    #[serde(default)]
    pub label: Option<String>,
    /// Optional gesture binding: `singleClick` or `doubleClick`.
    #[serde(default)]
    pub trigger: Option<String>,
}

/// Build a pet out of everything the user supplied.
///
/// Runs off the main thread: matting a batch is seconds to minutes of pixel
/// work and would freeze the pet mid-animation.
#[tauri::command]
async fn import_assets(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    name: String,
    assets: Vec<AssetIn>,
) -> Result<ImportResult, String> {
    use base64::Engine;
    use tauri::{Emitter, Manager};

    let progress_app = app.clone();
    let notify = move |msg: &str| {
        let _ = progress_app.emit_to("settings", "deskpet://import-progress", msg);
    };

    let run = || -> anyhow::Result<ImportResult> {
        use crate::core::import::{self, Matte};

        anyhow::ensure!(!assets.is_empty(), "nothing was added");
        anyhow::ensure!(
            assets.iter().any(|a| a.state == "idle"),
            "at least one asset has to be the idle pose — that is the one the \
             pet spends most of its time in"
        );

        let mut interactions = std::collections::HashMap::new();
        let mut action_labels = std::collections::HashMap::new();
        for asset in &assets {
            if let Some(trigger) = asset.trigger.as_deref() {
                anyhow::ensure!(
                    trigger == "singleClick" || trigger == "doubleClick",
                    "unsupported custom action trigger '{trigger}'"
                );
                anyhow::ensure!(
                    interactions
                        .insert(trigger.to_string(), asset.state.clone())
                        .is_none(),
                    "only one action can use {trigger}"
                );
                let label = asset.label.as_deref().unwrap_or("").trim();
                anyhow::ensure!(!label.is_empty(), "a custom action needs a name");
                action_labels.insert(asset.state.clone(), label.to_string());
            }
        }

        // Decode everything first, keeping track of which frames belong to
        // which state. Matting happens once, for the whole lot, because the
        // backend loads a 224 MB model per invocation.
        notify("reading files");
        let mut all: Vec<image::RgbaImage> = Vec::new();
        let mut spans: Vec<(String, usize, usize, bool)> = Vec::new();

        for asset in &assets {
            let start = all.len();
            for (i, b64) in asset.frames.iter().enumerate() {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64.as_bytes())
                    .with_context(|| {
                        format!("{} frame {i} did not survive the trip", asset.state)
                    })?;

                let img = if asset.is_clip {
                    // Webview-decoded PNG: no EXIF, already upright.
                    image::load_from_memory(&bytes)
                        .with_context(|| format!("{} frame {i}", asset.state))?
                        .to_rgba8()
                } else {
                    decode_photo(&bytes, &asset.state)?
                };

                all.push(match asset.rotate_degrees.rem_euclid(360) {
                    90 => image::imageops::rotate90(&img),
                    180 => image::imageops::rotate180(&img),
                    270 => image::imageops::rotate270(&img),
                    _ => img,
                });
            }
            anyhow::ensure!(all.len() > start, "{} had no usable frames", asset.state);
            spans.push((asset.state.clone(), start, all.len(), asset.is_clip));
        }

        // The idle asset goes first so it becomes the reference everything else
        // is scaled and colour-matched against.
        let idle_at = spans
            .iter()
            .position(|s| s.0 == "idle")
            .expect("checked above");
        if idle_at != 0 {
            reorder_idle_first(&mut all, &mut spans, idle_at);
        }

        let exe = std::env::current_exe()
            .context("cannot locate our own executable")?
            .parent()
            .context("our executable has no parent directory")?
            .join(format!("deskpet-assetpipe{}", std::env::consts::EXE_SUFFIX));
        let model_dir = app
            .path()
            .app_data_dir()
            .context("no app data directory")?
            .join("models");

        // Delivered alpha beats an inferred matte every time: whoever made the
        // asset knew where the subject was, and our model only guesses. See
        // docs/ASSET_SPEC.md §3.
        let pre_cut = all.iter().all(import::already_cut_out);

        let (cuts, backend) = if pre_cut {
            notify("using the transparency already in the files");
            tracing::info!("every asset arrived with alpha; skipping the matte");
            (all.clone(), "supplied alpha".to_string())
        } else {
            let matte = import::PipeMatte {
                exe,
                model_dir,
                on_progress: Box::new(notify.clone()),
            };
            let name = matte.name().to_string();
            (matte.cut_out_all(&all)?, name)
        };

        notify("aligning");
        let (mut placed, anchor) = import::fit_all(&cuts)?;
        import::match_colour(&mut placed, 0.6);

        // Assemble states out of the placed frames.
        let mut built: Vec<import::BuiltState> = Vec::new();
        for (state, start, end, is_clip) in &spans {
            let frames: Vec<image::RgbaImage> = placed[*start..*end].to_vec();

            // A single still gets procedural breathing so it is not a sticker.
            // A clip already moves and must not be second-guessed.
            let stable_loop = matches!(state.as_str(), "idle" | "sleep");
            let (frames, fps, looping) = if *is_clip {
                (frames, 12.0, stable_loop)
            } else if state.as_str() == "idle" {
                let f = import::breathe(&frames[0], &anchor, &[0.0, 0.5, 1.0, 0.5]);
                (f, 6.0, true)
            } else {
                (frames, 4.0, state.as_str() == "sleep")
            };

            built.push(import::BuiltState {
                name: state.clone(),
                frames: frames
                    .into_iter()
                    .enumerate()
                    .map(|(i, img)| (format!("{state}_{i:03}.png"), img))
                    .collect(),
                fps,
                looping,
            });
        }

        let root = app
            .path()
            .app_data_dir()
            .context("no app data directory")?
            .join("packs");

        // One id, reused: re-importing replaces the previous attempt instead of
        // filling the folder with half-liked pets.
        let pack_id = "imported".to_string();
        let sprites = root.join(&pack_id).join("sprites");
        if sprites.exists() {
            std::fs::remove_dir_all(&sprites).ok();
        }

        notify("writing the pack");
        let dir = import::write_pack(
            &root,
            &pack_id,
            &name,
            &built,
            &anchor,
            &interactions,
            &action_labels,
        )?;

        // Load it the same way boot does, so a bad pack fails while the user is
        // still looking at the settings window.
        let (loaded, masks) = crate::core::pack::load(&dir)?;
        *state.masks.lock().unwrap() = masks;

        {
            let mut cfg = state.config.lock().unwrap();
            cfg.active_pack = pack_id.clone();
            cfg.save(&app).ok();
        }

        let first = built
            .first()
            .and_then(|s| s.frames.first())
            .map(|(f, _)| f.clone())
            .unwrap_or_default();
        let preview = loaded.frames.get(&first).cloned().unwrap_or_default();
        let frames: usize = built.iter().map(|s| s.frames.len()).sum();

        app.emit_to("pet", "deskpet://pack-changed", ()).ok();
        tracing::info!(pack = %pack_id, states = built.len(), frames, "imported a pack");

        Ok(ImportResult {
            pack_id,
            backend,
            preview,
            frames,
        })
    };

    run().map_err(|e| {
        let msg = format!("{e:#}");
        tracing::error!("import failed: {msg}");
        msg
    })
}

/// Decode a photo, honouring EXIF orientation.
///
/// `image::load_from_memory` ignores the tag and hands back the sensor's raw
/// pixels, so every portrait phone photo arrives on its side.
fn decode_photo(bytes: &[u8], label: &str) -> anyhow::Result<image::RgbaImage> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .context("could not tell what kind of image that is")?;

    (|| -> image::ImageResult<image::RgbaImage> {
        let mut decoder = reader.into_decoder()?;
        let orientation = decoder.orientation()?;
        let mut img = image::DynamicImage::from_decoder(decoder)?;
        img.apply_orientation(orientation);
        Ok(img.to_rgba8())
    })()
    .with_context(|| {
        format!(
            "could not read the {label} image — PNG, JPEG and WebP are supported; \
                 HEIC photos need exporting as JPEG first"
        )
    })
}

/// Move the idle asset to the front so it is the reference for scale and
/// colour. Keeping the span bookkeeping in step is fiddly enough to be worth
/// its own function.
fn reorder_idle_first(
    all: &mut Vec<image::RgbaImage>,
    spans: &mut Vec<(String, usize, usize, bool)>,
    idle_at: usize,
) {
    let idle = spans.remove(idle_at);
    let idle_frames: Vec<image::RgbaImage> = all[idle.1..idle.2].to_vec();
    let len = idle.2 - idle.1;

    all.drain(idle.1..idle.2);
    for (i, frame) in idle_frames.into_iter().enumerate() {
        all.insert(i, frame);
    }

    let mut rebuilt = vec![(idle.0, 0usize, len, idle.3)];
    let mut cursor = len;
    for (name, start, end, is_clip) in spans.iter() {
        let span = end - start;
        rebuilt.push((name.clone(), cursor, cursor + span, *is_clip));
        cursor += span;
    }
    *spans = rebuilt;
}

/// Load the active pack: manifest to the frontend, hitmasks into shared state./// Load the active pack: manifest to the frontend, hitmasks into shared state.
///
/// The frontend calls this on boot and cannot render without it. Errors come
/// back as strings rather than being swallowed — a pack that fails to load is
/// something the user has to be told about, since they are the one who put it
/// there.
#[tauri::command]
fn load_active_pack(
    app: tauri::AppHandle,
    state: tauri::State<Arc<AppState>>,
) -> Result<crate::core::pack::LoadedPack, String> {
    let pack_id = state.config.lock().unwrap().active_pack.clone();

    let dir = crate::core::pack::resolve_dir(&app, &pack_id).map_err(|e| format!("{e:#}"))?;
    let (loaded, masks) = crate::core::pack::load(&dir).map_err(|e| format!("{e:#}"))?;

    // Swap the masks in one go. The hit-test loop takes this lock at 30 Hz, so
    // it must never observe a half-populated set.
    *state.masks.lock().unwrap() = masks;

    Ok(loaded)
}

#[tauri::command]
fn get_config(state: tauri::State<Arc<AppState>>) -> Config {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
fn set_sleep_after_seconds(
    app: tauri::AppHandle,
    state: tauri::State<Arc<AppState>>,
    seconds: u32,
) -> Result<(), String> {
    if !(5..=3600).contains(&seconds) {
        return Err("sleep time must be between 5 seconds and 60 minutes".into());
    }
    let mut cfg = state.config.lock().unwrap();
    cfg.sleep_after_seconds = seconds;
    cfg.save(&app)
        .map_err(|e| format!("saving sleep time: {e:#}"))?;
    drop(cfg);
    app.emit_to("pet", "deskpet://sleep-after-changed", seconds)
        .map_err(|e| format!("updating pet sleep time: {e}"))
}

#[tauri::command]
fn get_start_at_login() -> Result<bool, String> {
    platform::is_start_at_login_enabled().map_err(|e| format!("reading login startup: {e:#}"))
}

#[tauri::command]
fn set_start_at_login(
    app: tauri::AppHandle,
    state: tauri::State<Arc<AppState>>,
    enabled: bool,
) -> Result<bool, String> {
    platform::set_start_at_login(enabled).map_err(|e| format!("updating login startup: {e:#}"))?;

    let mut cfg = state.config.lock().unwrap();
    cfg.start_at_login = enabled;
    cfg.save(&app)
        .map_err(|e| format!("saving login startup preference: {e:#}"))?;
    Ok(enabled)
}

/// Remember where the user left the pet after a drag. Screen coordinates are
/// stored in physical pixels because that is what Tauri returns and it remains
/// unambiguous when monitors use different scale factors.
#[tauri::command]
fn save_window_position(
    app: tauri::AppHandle,
    state: tauri::State<Arc<AppState>>,
) -> Result<(), String> {
    let window = app
        .get_webview_window("pet")
        .ok_or_else(|| "pet window is missing".to_string())?;
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let mut cfg = state.config.lock().unwrap();
    cfg.window_x = Some(position.x);
    cfg.window_y = Some(position.y);
    cfg.save(&app)
        .map_err(|e| format!("saving pet position: {e:#}"))
}

/// Restore the user's placement, or put a new pet near the lower-right corner
/// with enough clearance for the Dock. The user can immediately drag it from
/// there; that placement becomes the next startup position.
fn place_pet_window(window: &WebviewWindow, cfg: &Config) -> anyhow::Result<()> {
    if let (Some(x), Some(y)) = (cfg.window_x, cfg.window_y) {
        window.set_position(tauri::PhysicalPosition::new(x, y))?;
        return Ok(());
    }

    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return Ok(());
    };
    let monitor_origin = monitor.position();
    let monitor_size = monitor.size();
    let window_size = window.outer_size()?;
    let scale = monitor.scale_factor();
    let right_margin = (24.0 * scale).round() as i32;
    let dock_clearance = (88.0 * scale).round() as i32;
    let x = monitor_origin.x + monitor_size.width as i32 - window_size.width as i32 - right_margin;
    let y =
        monitor_origin.y + monitor_size.height as i32 - window_size.height as i32 - dock_clearance;
    window.set_position(tauri::PhysicalPosition::new(x, y))?;
    Ok(())
}

/// Poll the cursor and toggle click-through.
///
/// Polling rather than reacting to mousemove is deliberate: a click-through
/// window receives no mouse events, so there is nothing to react to. 30 Hz is
/// below the threshold where the toggle feels laggy and cheap enough to be
/// invisible in Activity Monitor.
fn spawn_hit_test_loop(window: WebviewWindow, plat: Arc<Box<dyn PetWindow>>, state: Arc<AppState>) {
    std::thread::spawn(move || {
        let mut last_cursor = (f64::MIN, f64::MIN);
        let mut last_emit = std::time::Instant::now();
        let mut was_near = false;
        let mut was_over_pet = false;

        loop {
            std::thread::sleep(Duration::from_millis(33));

            // Never change click-through while a button is held.
            //
            // Dragging is the case that matters. `startDragging` hands the mouse
            // to AppKit's modal drag loop; meanwhile this loop samples the cursor
            // and the window origin at slightly different instants, and while the
            // window is being moved those two disagree. One bad sample puts the
            // cursor outside the mask, this loop sets `ignoresMouseEvents`, the
            // window stops receiving mouse events, and the drag loop never sees
            // its mouse-up — the pet freezes mid-drag, still stuck to the cursor.
            //
            // The general rule is the right one anyway: interactivity must not be
            // pulled out from under a press.
            if plat.mouse_button_down() {
                continue;
            }

            let Ok(cursor) = window.cursor_position() else {
                continue;
            };
            let Ok(origin) = window.outer_position() else {
                continue;
            };
            let scale = plat.scale_factor();

            let lx = (cursor.x - origin.x as f64) / scale;
            let ly = (cursor.y - origin.y as f64) / scale;

            {
                // No pack loaded means every hit test misses, which would pin the
                // window click-through and make the pet permanently unclickable.
                // Stay interactive instead — a pet you cannot click is worse than
                // one that swallows a click in its bounding box.
                if state.masks.lock().unwrap().is_empty() {
                    let _ = plat.set_click_through(false);
                    continue;
                }
            }

            let frame_id = state.frame.lock().unwrap().frame_id.clone();
            let within_frame_bounds = state.masks.lock().unwrap().within_bounds(&frame_id, lx, ly);

            // Feed the cursor to the renderer only inside the current frame's
            // tight visible bounds. Standing and lying poses occupy very
            // different parts of the fixed transparent canvas; the active mask
            // makes this response area contract automatically on every frame.
            //
            // The window is click-through over its empty corners, so the webview
            // never sees a mousemove out there — this loop is the only thing that
            // knows where the pointer is. Bounded deliberately: only while the
            // cursor is near enough to matter, at 15 Hz, and only when it actually
            // moved. A pet that emits an event 30 times a second all day is a pet
            // that shows up in Activity Monitor.
            {
                let near = within_frame_bounds;

                let moved = (lx - last_cursor.0).abs() > 1.5 || (ly - last_cursor.1).abs() > 1.5;
                let due = last_emit.elapsed() >= Duration::from_millis(66);

                if near && moved && due {
                    last_cursor = (lx, ly);
                    last_emit = std::time::Instant::now();
                    let _ = window.emit("deskpet://cursor", (lx, ly));
                } else if !near && was_near {
                    // One last event so the pet settles back to rest instead of
                    // holding whatever tilt it had when the cursor left.
                    let _ = window.emit("deskpet://cursor-away", ());
                }
                was_near = near;
            }

            let over_pet = state.masks.lock().unwrap().hit(&frame_id, lx, ly);

            // Behaviour reacts only when the cursor crosses the visible alpha
            // silhouette. The much wider `cursor` event above exists solely for
            // gaze/tilt and must not wake a sleeping pet from across the screen.
            if over_pet && !was_over_pet {
                let _ = window.emit("deskpet://pointer-over-pet", ());
            } else if !over_pet && was_over_pet {
                let _ = window.emit("deskpet://pointer-left-pet", ());
            }
            was_over_pet = over_pet;

            let _ = plat.set_click_through(!over_pet);
        }
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
        .invoke_handler(tauri::generate_handler![
            set_current_frame,
            get_config,
            set_sleep_after_seconds,
            get_start_at_login,
            set_start_at_login,
            save_window_position,
            frontend_log,
            load_active_pack,
            import_assets
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // No Dock slot, no Cmd-Tab entry, no application menu — the pet
            // lives on the desktop and in the menu bar. Quit is in the tray
            // menu, which this makes the only way out.
            if let Err(e) = platform::hide_from_dock() {
                tracing::warn!("could not hide from the Dock: {e:#}");
            }

            // Still worth setting: an accessory app has no Dock icon, but the
            // system still shows this icon in permission prompts and
            // notifications.
            if let Err(e) = platform::set_app_icon(include_bytes!("../icons/icon.png")) {
                tracing::warn!("could not set the app icon: {e:#}");
            }

            let mut cfg = Config::load(&handle);
            if cfg.webhook_token.is_empty() {
                cfg.webhook_token = webhook::generate_token();
                cfg.save(&handle).ok();
            }
            if cfg.start_at_login {
                // Refresh the executable path after an app update or move.
                if let Err(e) = platform::set_start_at_login(true) {
                    tracing::warn!("could not refresh login startup registration: {e:#}");
                }
            }

            // One AppState, shared. The worker loops and the command handlers
            // must see the same masks and the same frame id; two instances
            // meant the hit-test loop was reading a map nothing ever wrote to.
            let state = Arc::new(AppState {
                masks: Mutex::new(MaskSet::default()),
                frame: Mutex::new(FrameState::default()),
                config: Mutex::new(cfg.clone()),
            });
            app.manage(state.clone());

            let window = app
                .get_webview_window("pet")
                .expect("window 'pet' missing from tauri.conf.json");

            // Everything OS-specific happens behind this call.
            let plat = Arc::new(platform::attach(&window)?);
            plat.configure()?;
            plat.set_stack_level(StackLevel::Floating)?;
            window.show()?;
            // `current_monitor()` can be absent while the window is hidden on
            // macOS, so initial placement belongs after the first show.
            if let Err(e) = place_pet_window(&window, &cfg) {
                tracing::warn!("could not place pet window: {e:#}");
            }
            // "The process did not crash" is not the same claim as "the pet is
            // on the screen". Say which one actually happened.
            eprintln!("[deskpet] window after show: {}", plat.debug_report());

            spawn_hit_test_loop(window.clone(), plat.clone(), state.clone());
            spawn_environment_loop(window.clone(), plat.clone(), state.clone());

            let quit = MenuItem::with_id(app, "quit", "Quit DeskPet", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings, &quit])?;

            // The menu bar icon is embedded rather than read from the bundle:
            // a tray item with no icon is an invisible blank slot in the menu
            // bar, which reads as a broken app, and that failure mode should
            // not depend on resource packaging being right.
            //
            // `tray.png` is a template image — black plus alpha, no colour —
            // so macOS tints it for light, dark and highlighted menu bars.
            // `icon_as_template(true)` is what asks for that treatment; a
            // coloured icon here looks fine until the appearance changes.
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;

            TrayIconBuilder::new()
                .icon(tray_icon)
                .icon_as_template(true)
                .tooltip("DeskPet")
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
                if let Err(e) = webhook::serve(hook_app, cfg.webhook_token, cfg.webhook_port).await
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
    // An accessory app is not activated by clicking, so a window opened from
    // the tray would appear behind whatever the user was looking at and would
    // not take keystrokes. They just asked for it; bring the app forward.
    let _ = platform::activate_app();

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
    .inner_size(820.0, 720.0)
    .resizable(true)
    .build();
}
