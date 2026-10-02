//! Loading a pet pack from disk.
//!
//! The contract is `docs/PETPACK_SPEC.md`; this module is its Rust half. If
//! the two disagree, the spec wins and this file is the bug.
//!
//! Two things happen here that the frontend cannot do for itself:
//!
//!   1. **Hitmasks.** Built from frame alpha at load time and kept in Rust,
//!      because the click-through poll loop runs at 30 Hz on a worker thread
//!      and cannot round-trip to the webview for every mouse position.
//!   2. **Validation.** A pack is user-supplied content. Every reference in it
//!      — frame files, transition targets, sourceMap targets — is checked
//!      before anything is loaded, so a broken pack produces one clear message
//!      instead of a half-loaded pet.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::hitmask::{HitMask, MaskSet, DEFAULT_DOWNSCALE};

/// The only format version this build understands.
pub const SUPPORTED_FORMAT: u32 = 0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Anchor {
    /// The character's foot line, in canvas coordinates. Frames align on this
    /// and `centroid_x`, never on canvas centre — bounding-box centring makes
    /// the character bob whenever a limb extends.
    pub baseline_y: f32,
    pub centroid_x: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDef {
    pub frames: Vec<String>,
    pub fps: f32,
    /// `loop` is a Rust keyword; the wire name stays `loop` per the spec.
    #[serde(rename = "loop", default)]
    pub loop_: Option<bool>,
    /// State to fall into when a non-looping animation finishes.
    #[serde(default)]
    pub next: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transition {
    pub to: String,
    #[serde(default)]
    pub weight: Option<f32>,
    #[serde(default)]
    pub min_boredom: Option<f32>,
    #[serde(default)]
    pub max_boredom: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eye {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gaze {
    pub enabled: bool,
    #[serde(default)]
    pub eyes: Vec<Eye>,
    #[serde(default)]
    pub max_offset: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shadow {
    pub enabled: bool,
    pub opacity: f32,
    pub width_ratio: f32,
    pub blur: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    pub id: String,
    pub name: String,
    pub author: String,
    pub license: String,
    pub canvas: Canvas,
    pub anchor: Anchor,
    pub states: HashMap<String, StateDef>,
    #[serde(default)]
    pub transitions: HashMap<String, Vec<Transition>>,
    #[serde(default)]
    pub interactions: HashMap<String, String>,
    #[serde(default)]
    pub action_labels: HashMap<String, String>,
    #[serde(default)]
    pub gaze: Option<Gaze>,
    #[serde(default)]
    pub shadow: Option<Shadow>,
    #[serde(default)]
    pub source_map: HashMap<String, String>,
}

/// What the frontend receives from `load_active_pack`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedPack {
    pub manifest: Manifest,
    /// Frame id (the filename as written in the manifest) -> `data:` URL.
    ///
    /// Frames are inlined rather than served over the `asset:` protocol so
    /// that the asset scope does not have to be widened to cover an
    /// arbitrary user pack directory. At v0 canvas sizes a whole pack is a
    /// few hundred kilobytes; if packs grow, this is the one place to switch.
    pub frames: HashMap<String, String>,
    /// Where the pack was loaded from. Shown in settings; also the thing to
    /// look at first when someone says "it loaded the wrong pet".
    pub source_dir: String,
}

impl StateDef {
    pub fn looping(&self) -> bool {
        self.loop_.unwrap_or(true)
    }
}

/// Search order for `packs/<id>`:
///
///   1. the user's data dir — packs they imported or installed
///   2. the bundled resource dir — packs shipped with the app
///   3. (debug builds only) up the tree from the crate, for `cargo tauri dev`
///
/// User packs shadow bundled ones deliberately: that is how someone replaces
/// the default pet without deleting anything.
pub fn resolve_dir(app: &tauri::AppHandle, pack_id: &str) -> Result<PathBuf> {
    use tauri::Manager;

    let mut tried: Vec<PathBuf> = Vec::new();

    if let Ok(dir) = app.path().app_data_dir() {
        let p = dir.join("packs").join(pack_id);
        if p.is_dir() {
            return Ok(p);
        }
        tried.push(p);
    }

    if let Ok(dir) = app.path().resource_dir() {
        let p = dir.join("packs").join(pack_id);
        if p.is_dir() {
            return Ok(p);
        }
        tried.push(p);
    }

    #[cfg(debug_assertions)]
    {
        // `cargo tauri dev` runs from apps/desktop/src-tauri with no bundled
        // resources, so walk up to the repo root and use packs/ there.
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let p = dir.join("packs").join(pack_id);
            if p.is_dir() {
                return Ok(p);
            }
            tried.push(p);
            if !dir.pop() {
                break;
            }
        }
    }

    bail!(
        "pack '{pack_id}' not found. Looked in:\n  {}",
        tried
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n  ")
    )
}

/// Read, validate and decode a pack directory.
///
/// Returns what the frontend needs plus the hitmasks for the poll loop. Both
/// come from the same pass over the frames, so a frame can never be drawn
/// without a mask or vice versa.
pub fn load(dir: &Path) -> Result<(LoadedPack, MaskSet)> {
    let manifest_path = dir.join("manifest.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest: Manifest = serde_json::from_str(&raw)
        .with_context(|| format!("parsing {}", manifest_path.display()))?;

    validate(&manifest)?;

    let sprites = dir.join("sprites");
    let mut frames = HashMap::new();
    let mut masks = MaskSet::default();

    for frame_id in unique_frames(&manifest) {
        let path = sprites.join(&frame_id);
        let bytes = std::fs::read(&path).with_context(|| {
            format!(
                "state references '{frame_id}' but {} does not exist",
                path.display()
            )
        })?;

        // Decode once for the mask. The bytes go to the webview untouched —
        // re-encoding would cost time and lose nothing but quality.
        let img = image::load_from_memory(&bytes)
            .with_context(|| format!("decoding {}", path.display()))?
            .to_rgba8();

        let (w, h) = img.dimensions();
        if w != manifest.canvas.width || h != manifest.canvas.height {
            bail!(
                "{frame_id} is {w}x{h} but the manifest declares a \
                 {}x{} canvas. Every frame must match the canvas exactly — \
                 the hit test and the anchor both work in canvas coordinates.",
                manifest.canvas.width,
                manifest.canvas.height
            );
        }

        masks.insert(
            frame_id.clone(),
            HitMask::from_rgba(img.as_raw(), w, h, DEFAULT_DOWNSCALE),
        );
        frames.insert(frame_id.clone(), data_url(&frame_id, &bytes)?);
    }

    tracing::info!(
        pack = %manifest.id,
        frames = frames.len(),
        mask_bytes = masks.total_bytes(),
        "pack loaded"
    );

    Ok((
        LoadedPack {
            manifest,
            frames,
            source_dir: dir.display().to_string(),
        },
        masks,
    ))
}

/// Every reference in the manifest must resolve. A pack is user content;
/// failing loudly here beats a pet that is silently missing a state.
fn validate(m: &Manifest) -> Result<()> {
    if m.format != SUPPORTED_FORMAT {
        bail!(
            "pack format {} is not supported by this build (expected {})",
            m.format,
            SUPPORTED_FORMAT
        );
    }
    if m.canvas.width == 0 || m.canvas.height == 0 {
        bail!("canvas must have a non-zero width and height");
    }
    if !m.states.contains_key("idle") {
        bail!("every pack must define an 'idle' state");
    }

    for (name, def) in &m.states {
        if def.frames.is_empty() {
            bail!("state '{name}' has no frames");
        }
        if def.fps <= 0.0 {
            bail!("state '{name}' has fps {} — must be positive", def.fps);
        }
        if let Some(next) = &def.next {
            if !m.states.contains_key(next) {
                bail!("state '{name}' falls through to '{next}', which does not exist");
            }
        }
    }

    for (from, list) in &m.transitions {
        if !m.states.contains_key(from) {
            bail!("transitions declared for '{from}', which is not a state");
        }
        for t in list {
            if !m.states.contains_key(&t.to) {
                bail!(
                    "transition '{from}' -> '{}' targets a state that does not exist",
                    t.to
                );
            }
        }
    }

    for (source, state) in &m.source_map {
        if !m.states.contains_key(state) {
            bail!("sourceMap '{source}' -> '{state}' targets a state that does not exist");
        }
    }

    for (gesture, state) in &m.interactions {
        if gesture != "singleClick" && gesture != "doubleClick" {
            bail!("interaction '{gesture}' is not supported");
        }
        if !m.states.contains_key(state) {
            bail!("interaction '{gesture}' targets '{state}', which does not exist");
        }
    }

    for state in m.action_labels.keys() {
        if !m.states.contains_key(state) {
            bail!("action label targets '{state}', which does not exist");
        }
    }

    Ok(())
}

/// Frames deduplicated across states — packs commonly reuse one frame in
/// several states, and decoding it more than once is wasted work.
fn unique_frames(m: &Manifest) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for def in m.states.values() {
        for f in &def.frames {
            seen.insert(f.clone());
        }
    }
    seen.into_iter().collect()
}

fn data_url(frame_id: &str, bytes: &[u8]) -> Result<String> {
    use base64::Engine;
    let mime = match Path::new(frame_id)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("webp") => "image/webp",
        Some("png") => "image/png",
        other => bail!("frame '{frame_id}' has extension {other:?}; packs use WebP or PNG"),
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(test)]
mod manifest_tests {
    use super::*;

    fn manifest(extra: &str) -> Manifest {
        let source = format!(
            r#"{{
              "format": 0,
              "id": "test.pet",
              "name": "Test",
              "author": "Test",
              "license": "Test",
              "canvas": {{ "width": 256, "height": 256 }},
              "anchor": {{ "baselineY": 240, "centroidX": 128 }},
              "states": {{ "idle": {{ "frames": ["idle.webp"], "fps": 1, "loop": true }} }}
              {extra}
            }}"#
        );
        serde_json::from_str(&source).unwrap()
    }

    #[test]
    fn old_manifest_without_interactions_is_valid() {
        validate(&manifest("")).unwrap();
    }

    #[test]
    fn interaction_must_target_an_existing_state() {
        let err = validate(&manifest(r#", "interactions": { "singleClick": "run" }"#))
            .unwrap_err()
            .to_string();
        assert!(err.contains("does not exist"));
    }
}
