//! Turning a photo into a pack.
//!
//! Everything here is independent of how the cutout is produced. The matte
//! backend (macOS Vision, or BiRefNet via ONNX) plugs into `Matte` and nothing
//! else in this file changes when it is swapped.
//!
//! ## Anchor, and why it is not the bounding box
//!
//! Frames align on the foot baseline and the horizontal centroid of the opaque
//! pixels, never on the canvas centre or the bounding box. Bounding-box
//! centring makes the character bob every time a limb extends, which is the
//! most common source of jitter in hand-assembled packs (PETPACK_SPEC.md).
//! Computing it here, from alpha, means an imported photo gets it right
//! without the user knowing the concept exists.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use image::{imageops, RgbaImage};

/// Canvas edge for imported packs. Matches the default pack.
pub const CANVAS: u32 = 256;
/// Fraction of the canvas height left below the character's feet.
const FLOOR_MARGIN: f32 = 0.08;
/// Pixels at or below this alpha are treated as empty when measuring.
const ALPHA_FLOOR: u8 = 24;

/// Produces a cutout: same dimensions in, alpha channel meaningful out.
///
/// Deliberately not a trait object over the network or the filesystem — a
/// matte backend gets pixels and returns pixels, so it can be tested without
/// a model, a GPU, or a window.
pub trait Matte {
    /// Cut out every frame in one go.
    ///
    /// Batch rather than per-image because the backend loads a 224 MB model:
    /// per-frame calls would spend all their time on setup. A clip is dozens
    /// of frames, so this is the difference between seconds and minutes.
    fn cut_out_all(&self, imgs: &[RgbaImage]) -> Result<Vec<RgbaImage>>;
    /// Shown in logs and in the settings UI, so the user can tell which
    /// backend produced a disappointing result.
    fn name(&self) -> &'static str;
}

/// Runs the real matte: `deskpet-assetpipe`, one process per import.
///
/// The model is 224 MB and the runtime that executes it has no business living
/// in a process that is supposed to idle under 60 MB. Spawning means the app's
/// dependency tree never contains `ort`, and a crash inside inference cannot
/// take the pet down with it. See HANDOFF.md, "Where the model runs".
///
/// Pixels cross the boundary as PNG files in the temp directory, which is the
/// price of the isolation and is cheap next to the inference itself.
pub struct PipeMatte {
    pub exe: PathBuf,
    pub model_dir: PathBuf,
    /// Called with each `progress:` line the pipeline emits.
    ///
    /// The first import downloads 224 MB and can take minutes. A blocking call
    /// with no output would be indistinguishable from a hang, and the user
    /// would kill it — so progress is plumbed all the way to the settings
    /// window rather than swallowed here.
    pub on_progress: Box<dyn Fn(&str) + Send + Sync>,
}

impl Matte for PipeMatte {
    fn name(&self) -> &'static str {
        "BiRefNet_lite (assetpipe)"
    }

    fn cut_out_all(&self, imgs: &[RgbaImage]) -> Result<Vec<RgbaImage>> {
        if !self.exe.is_file() {
            bail!(
                "the cutout pipeline is missing at {}. Build it with \
                 `cargo build -p deskpet-assetpipe`.",
                self.exe.display()
            );
        }

        // Unique names: two imports at once must not read each other's files.
        anyhow::ensure!(!imgs.is_empty(), "nothing to cut out");

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let work = std::env::temp_dir().join(format!("deskpet-{stamp}"));
        let in_path = work.join("in");
        let out_path = work.join("out");
        std::fs::create_dir_all(&in_path)
            .with_context(|| format!("creating {}", in_path.display()))?;

        // Zero-padded so lexical order is frame order on the other side.
        for (i, img) in imgs.iter().enumerate() {
            img.save(in_path.join(format!("{i:05}.png")))
                .with_context(|| format!("writing frame {i}"))?;
        }

        let spawned = std::process::Command::new(&self.exe)
            .arg("cutout")
            .arg("--input")
            .arg(&in_path)
            .arg("--output")
            .arg(&out_path)
            .arg("--model-dir")
            .arg(&self.model_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .with_context(|| format!("running {}", self.exe.display()));

        let outcome = (|| -> Result<()> {
            let mut child = spawned?;

            // Read stderr as it arrives. Waiting first and reading after would
            // both lose the progress and risk filling the pipe buffer on a long
            // download, which deadlocks the child.
            let stderr = child.stderr.take().context("no stderr pipe")?;
            let mut tail: Vec<String> = Vec::new();
            for line in std::io::BufRead::lines(std::io::BufReader::new(stderr)) {
                let line = line.unwrap_or_default();
                if let Some(msg) = line.strip_prefix("progress: ") {
                    (self.on_progress)(msg);
                } else if !line.trim().is_empty() {
                    // Keep a bounded tail: a failing model load can be chatty
                    // and none of it is worth unbounded memory.
                    tail.push(line);
                    if tail.len() > 40 {
                        tail.remove(0);
                    }
                }
            }

            let status = child.wait().context("waiting for the cutout pipeline")?;
            if !status.success() {
                // The pipeline writes a full anyhow chain. Pass it through
                // rather than replacing it with "import failed" — the useful
                // sentence is in there.
                let detail = tail
                    .iter()
                    .rev()
                    .find(|l| l.starts_with("error:"))
                    .cloned()
                    .unwrap_or_else(|| tail.join("; "));
                bail!("cutout failed: {detail}");
            }
            Ok(())
        })();

        if let Err(e) = outcome {
            std::fs::remove_dir_all(&work).ok();
            return Err(e);
        }

        let mut cuts = Vec::with_capacity(imgs.len());
        for i in 0..imgs.len() {
            let p = out_path.join(format!("{i:05}.png"));
            let cut = image::open(&p)
                .with_context(|| format!("reading the cutout at {}", p.display()))?
                .to_rgba8();
            cuts.push(cut);
        }

        std::fs::remove_dir_all(&work).ok();
        Ok(cuts)
    }
}

/// True when the image already carries a usable alpha channel.
///
/// A generator that produced the picture knows exactly where its subject is;
/// a matte model only guesses. When alpha is delivered we use it and skip the
/// cutout entirely — faster, and strictly more accurate than anything we could
/// infer afterwards.
///
/// "Usable" means genuinely mixed: a fully opaque image (a photograph) and a
/// fully transparent one (a mistake) both fail. The thresholds are loose
/// because a legitimate cutout can be almost any shape.
pub fn already_cut_out(img: &RgbaImage) -> bool {
    let mut clear = 0usize;
    let mut solid = 0usize;
    for px in img.pixels() {
        match px.0[3] {
            0..=8 => clear += 1,
            240..=255 => solid += 1,
            _ => {}
        }
    }
    let total = (img.width() * img.height()) as usize;
    if total == 0 {
        return false;
    }
    // At least a twentieth transparent and a twentieth opaque.
    clear * 20 > total && solid * 20 > total
}

/// Where the character stands, measured from alpha./// Where the character stands, measured from alpha.
#[derive(Debug, Clone, Copy)]
pub struct Anchor {
    pub baseline_y: f32,
    pub centroid_x: f32,
}

/// Tight box of the opaque pixels, plus the anchor.
///
/// `baseline_y` is the lowest opaque row — the feet. `centroid_x` is the
/// alpha-weighted horizontal centre of mass, not the middle of the bounding
/// box: a character holding one arm out should not be shifted sideways
/// because of it.
pub fn measure(img: &RgbaImage) -> Result<(u32, u32, u32, u32, Anchor)> {
    let (w, h) = img.dimensions();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (u32::MAX, u32::MAX, 0u32, 0u32);
    let mut weight_sum = 0f64;
    let mut weighted_x = 0f64;

    for y in 0..h {
        for x in 0..w {
            let a = img.get_pixel(x, y).0[3];
            if a <= ALPHA_FLOOR {
                continue;
            }
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            let wgt = a as f64 / 255.0;
            weight_sum += wgt;
            weighted_x += wgt * x as f64;
        }
    }

    if weight_sum == 0.0 {
        bail!("the cutout is completely transparent — nothing was kept as foreground");
    }

    Ok((
        min_x,
        min_y,
        max_x,
        max_y,
        Anchor {
            baseline_y: max_y as f32,
            centroid_x: (weighted_x / weight_sum) as f32,
        },
    ))
}

/// A pose-independent size for one cutout: the square root of its opaque
/// area, weighted by alpha.
///
/// Height is the obvious choice and the wrong one — a dog lying down is a
/// third as tall as the same dog standing, so normalising on height would
/// inflate every lying pose into a giant. Area barely changes with pose.
pub fn size_proxy(img: &RgbaImage) -> f32 {
    let mut sum = 0f64;
    for px in img.pixels() {
        sum += px.0[3] as f64 / 255.0;
    }
    (sum.max(1.0)).sqrt() as f32
}

/// Place one cutout on the pack canvas at a given scale, standing on the floor.
fn place(cut: &RgbaImage, scale: f32) -> Result<(RgbaImage, Anchor)> {
    let (min_x, min_y, max_x, max_y, _) = measure(cut)?;
    let sub_w = max_x - min_x + 1;
    let sub_h = max_y - min_y + 1;
    let cropped = imageops::crop_imm(cut, min_x, min_y, sub_w, sub_h).to_image();

    let dst_w = ((sub_w as f32 * scale).round() as u32).max(1);
    let dst_h = ((sub_h as f32 * scale).round() as u32).max(1);
    let resized = imageops::resize(&cropped, dst_w, dst_h, imageops::FilterType::Lanczos3);

    let floor = (CANVAS as f32 * (1.0 - FLOOR_MARGIN)).round() as i64;
    let mut canvas = RgbaImage::new(CANVAS, CANVAS);
    imageops::overlay(
        &mut canvas,
        &resized,
        (CANVAS as i64 - dst_w as i64) / 2,
        floor - dst_h as i64,
    );

    // Re-measure on the canvas: the resize resamples alpha, so the feet can
    // move a pixel and the anchor has to describe what is actually there.
    let (_, _, _, _, anchor) = measure(&canvas)?;
    Ok((canvas, anchor))
}

/// Fit every cutout onto the canvas at a **common** scale.
///
/// The reference is the first frame: it is fitted to the canvas, and every
/// other frame is scaled so the animal is the same size in all of them. Doing
/// it per frame instead — fitting each one to the canvas on its own — is what
/// makes an imported pet look like a slideshow: it grows when it sits and
/// shrinks when it stretches.
pub fn fit_all(cuts: &[RgbaImage]) -> Result<(Vec<RgbaImage>, Anchor)> {
    anyhow::ensure!(!cuts.is_empty(), "no cutouts to fit");

    let reference = &cuts[0];
    let (rx0, ry0, rx1, ry1, _) = measure(reference)?;
    let ref_w = (rx1 - rx0 + 1) as f32;
    let ref_h = (ry1 - ry0 + 1) as f32;

    let floor = CANVAS as f32 * (1.0 - FLOOR_MARGIN);
    let ref_scale = (CANVAS as f32 / ref_w).min(floor / ref_h);
    let target = size_proxy(reference) * ref_scale;

    // Per-frame scale that puts every animal at the same size.
    let mut scales: Vec<f32> = cuts
        .iter()
        .map(|c| target / size_proxy(c).max(1.0))
        .collect();

    // A pose can still be too wide or tall for the canvas at that scale — a
    // stretching cat is much longer than a sitting one. Shrink everything by
    // the same factor rather than that one frame, or the size normalisation we
    // just did is undone.
    let mut correction = 1.0f32;
    for (c, scale) in cuts.iter().zip(&scales) {
        let (x0, y0, x1, y1, _) = measure(c)?;
        let w = (x1 - x0 + 1) as f32 * scale;
        let h = (y1 - y0 + 1) as f32 * scale;
        correction = correction
            .min(CANVAS as f32 / w.max(1.0))
            .min(floor / h.max(1.0));
    }
    if correction < 1.0 {
        for scale in &mut scales {
            *scale *= correction;
        }
    }

    let mut placed = Vec::with_capacity(cuts.len());
    let mut anchor = None;
    for (c, scale) in cuts.iter().zip(&scales) {
        let (img, a) = place(c, *scale)?;
        if anchor.is_none() {
            anchor = Some(a);
        }
        placed.push(img);
    }

    Ok((placed, anchor.expect("at least one frame")))
}

/// Nudge every frame's colour toward the reference.
///
/// Photos taken indoors and outdoors differ enough that switching between them
/// flickers. Matching the mean of the opaque pixels damps that. Deliberately
/// partial: a full match would flatten genuine differences, like a pet lit by a
/// sunset, into a wrong-looking average.
pub fn match_colour(frames: &mut [RgbaImage], strength: f32) {
    if frames.len() < 2 {
        return;
    }
    let Some(reference) = mean_rgb(&frames[0]) else {
        return;
    };

    for frame in frames.iter_mut().skip(1) {
        let Some(mean) = mean_rgb(frame) else {
            continue;
        };
        let shift = [
            (reference[0] - mean[0]) * strength,
            (reference[1] - mean[1]) * strength,
            (reference[2] - mean[2]) * strength,
        ];
        for px in frame.pixels_mut() {
            if px.0[3] == 0 {
                continue;
            }
            for (c, delta) in shift.iter().enumerate() {
                px.0[c] = (px.0[c] as f32 + delta).clamp(0.0, 255.0) as u8;
            }
        }
    }
}

fn mean_rgb(img: &RgbaImage) -> Option<[f32; 3]> {
    let mut sum = [0f64; 3];
    let mut weight = 0f64;
    for px in img.pixels() {
        let a = px.0[3] as f64 / 255.0;
        if a <= 0.1 {
            continue;
        }
        weight += a;
        for (c, channel_sum) in sum.iter_mut().enumerate() {
            *channel_sum += px.0[c] as f64 * a;
        }
    }
    if weight <= 0.0 {
        return None;
    }
    Some([
        (sum[0] / weight) as f32,
        (sum[1] / weight) as f32,
        (sum[2] / weight) as f32,
    ])
}

/// Breathing frames from a single still./// Breathing frames from a single still.
///
/// A one-frame pack is valid, but a character that never moves reads as a
/// sticker no matter how good the cutout is. Squashing vertically while
/// widening slightly keeps volume roughly conserved, which is the difference
/// between "breathing" and "being scaled".
pub fn breathe(base: &RgbaImage, anchor: &Anchor, steps: &[f32]) -> Vec<RgbaImage> {
    steps
        .iter()
        .map(|&t| {
            if t == 0.0 {
                return base.clone();
            }
            let (w, h) = base.dimensions();
            let sy = 1.0 - 0.035 * t;
            let sx = 1.0 + 0.025 * t;
            let nw = ((w as f32 * sx).round() as u32).max(1);
            let nh = ((h as f32 * sy).round() as u32).max(1);
            let scaled = imageops::resize(base, nw, nh, imageops::FilterType::Lanczos3);

            // Keep the feet planted: align the bottom of the scaled image to
            // the baseline instead of centring it, or the character bobs.
            let mut out = RgbaImage::new(w, h);
            let off_x = (w as i64 - nw as i64) / 2;
            let off_y =
                anchor.baseline_y as i64 - nh as i64 + (h as i64 - anchor.baseline_y as i64).min(0);
            imageops::overlay(&mut out, &scaled, off_x, off_y.max(0));
            out
        })
        .collect()
}

/// One state's worth of finished frames, ready to be written.
pub struct BuiltState {
    pub name: String,
    /// `(file name, image)`, in playback order.
    pub frames: Vec<(String, RgbaImage)>,
    pub fps: f32,
    pub looping: bool,
}

/// Write a pack directory: manifest, sprites, LICENSE.
pub fn write_pack(
    root: &Path,
    id: &str,
    name: &str,
    states: &[BuiltState],
    anchor: &Anchor,
    interactions: &std::collections::HashMap<String, String>,
    action_labels: &std::collections::HashMap<String, String>,
) -> Result<PathBuf> {
    anyhow::ensure!(
        states.iter().any(|s| s.name == "idle"),
        "a pack needs an idle state; nothing was assigned to it"
    );

    let dir = root.join(id);
    let sprites = dir.join("sprites");
    std::fs::create_dir_all(&sprites).with_context(|| format!("creating {}", sprites.display()))?;

    for state in states {
        for (file, img) in &state.frames {
            img.save(sprites.join(file))
                .with_context(|| format!("writing {file}"))?;
        }
    }

    let mut states_json = serde_json::Map::new();
    for state in states {
        let files: Vec<&str> = state.frames.iter().map(|(f, _)| f.as_str()).collect();
        let mut def = serde_json::json!({
            "frames": files,
            "fps": state.fps,
            "loop": state.looping,
        });
        // A one-shot state has to say where it goes afterwards or the pet gets
        // stuck in it — the trap that froze the default pet on first click.
        if !state.looping {
            def["next"] = serde_json::json!(if state.name == "lie_down" {
                "sleep"
            } else {
                "idle"
            });
        }
        states_json.insert(state.name.clone(), def);
    }

    // Only wire transitions between states that exist. A manifest that points
    // at a missing state fails validation at load, which would turn a partial
    // import into a pet that will not open at all.
    let has = |n: &str| states.iter().any(|s| s.name == n);
    let transitions = serde_json::Map::new();

    // Computed outside the macro: `json!` parses value positions and will not
    // take an `if` expression there.
    let source_map = if has("notify") {
        serde_json::json!({ "default": "notify" })
    } else {
        serde_json::json!({})
    };

    let manifest = serde_json::json!({
        "format": 0,
        "id": id,
        "name": name,
        "author": "imported",
        "license": "All rights reserved by the importer",
        "canvas": { "width": CANVAS, "height": CANVAS },
        "anchor": { "baselineY": anchor.baseline_y, "centroidX": anchor.centroid_x },
        "states": states_json,
        "transitions": transitions,
        "interactions": interactions,
        "actionLabels": action_labels,
        "gaze": { "enabled": false, "eyes": [], "maxOffset": 0 },
        "shadow": { "enabled": true, "opacity": 0.22, "widthRatio": 0.6, "blur": 8 },
        "sourceMap": source_map,
    });

    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    // The spec requires a LICENSE. An imported photo belongs to whoever
    // imported it; say so rather than inventing a licence for their picture.
    std::fs::write(
        dir.join("LICENSE"),
        "This pack was generated from material supplied by the user of this \
         machine.\nIt carries whatever rights that material already carried. It \
         is not redistributable\nby default — do not publish it without checking.\n",
    )?;

    Ok(dir)
}
