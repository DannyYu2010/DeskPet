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
use image::{imageops, GenericImageView, RgbaImage};

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
    fn cut_out(&self, img: &RgbaImage) -> Result<RgbaImage>;
    /// Shown in logs and in the settings UI, so the user can tell which
    /// backend produced a disappointing result.
    fn name(&self) -> &'static str;
}

/// Stand-in matte: flood the background inward from the border.
///
/// This is **not** the real thing and is not meant to be kept. It exists so
/// the rest of the pipeline — import, anchor detection, frame generation, pack
/// writing, hot-swapping the active pack — can be built and verified on real
/// hardware before a 100 MB model download is added to the story. When the
/// cutout looks bad, that is expected; when the *pipeline* looks bad, that is
/// a bug worth chasing.
///
/// Works acceptably on a photo shot against a plain wall and badly on
/// everything else, which is a fair description of what it is for.
pub struct BorderFloodMatte {
    /// Squared RGB distance under which a pixel counts as "same as the
    /// background colour it grew from".
    pub tolerance: i32,
}

impl Default for BorderFloodMatte {
    fn default() -> Self {
        Self { tolerance: 2500 } // ~50 per channel
    }
}

impl Matte for BorderFloodMatte {
    fn name(&self) -> &'static str {
        "border-flood (placeholder)"
    }

    fn cut_out(&self, img: &RgbaImage) -> Result<RgbaImage> {
        let (w, h) = img.dimensions();
        if w == 0 || h == 0 {
            bail!("empty image");
        }

        // Seed from the four corners rather than one, so a gradient background
        // does not strand a corner as "foreground".
        let mut out = img.clone();
        let mut visited = vec![false; (w * h) as usize];
        let mut queue: std::collections::VecDeque<(u32, u32, [u8; 3])> = Default::default();

        for (sx, sy) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
            let p = img.get_pixel(sx, sy).0;
            queue.push_back((sx, sy, [p[0], p[1], p[2]]));
        }

        while let Some((x, y, seed)) = queue.pop_front() {
            let idx = (y * w + x) as usize;
            if visited[idx] {
                continue;
            }
            let p = img.get_pixel(x, y).0;
            let d = (p[0] as i32 - seed[0] as i32).pow(2)
                + (p[1] as i32 - seed[1] as i32).pow(2)
                + (p[2] as i32 - seed[2] as i32).pow(2);
            if d > self.tolerance {
                continue;
            }
            visited[idx] = true;
            out.get_pixel_mut(x, y).0[3] = 0;

            if x > 0 {
                queue.push_back((x - 1, y, seed));
            }
            if x + 1 < w {
                queue.push_back((x + 1, y, seed));
            }
            if y > 0 {
                queue.push_back((x, y - 1, seed));
            }
            if y + 1 < h {
                queue.push_back((x, y + 1, seed));
            }
        }

        Ok(out)
    }
}

/// Where the character stands, measured from alpha.
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

/// Scale the cutout into the pack canvas and stand it on the floor line.
///
/// Returns the canvas image and the anchor expressed in canvas coordinates.
pub fn fit_to_canvas(cut: &RgbaImage) -> Result<(RgbaImage, Anchor)> {
    let (min_x, min_y, max_x, max_y, anchor) = measure(cut)?;

    let sub_w = max_x - min_x + 1;
    let sub_h = max_y - min_y + 1;
    let cropped = imageops::crop_imm(cut, min_x, min_y, sub_w, sub_h).to_image();

    // Leave a margin under the feet so the contact shadow has somewhere to go.
    let floor = (CANVAS as f32 * (1.0 - FLOOR_MARGIN)).round() as u32;
    let max_h = floor;
    let scale = (CANVAS as f32 / sub_w as f32).min(max_h as f32 / sub_h as f32);
    let dst_w = ((sub_w as f32 * scale).round() as u32).max(1);
    let dst_h = ((sub_h as f32 * scale).round() as u32).max(1);

    let resized = imageops::resize(&cropped, dst_w, dst_h, imageops::FilterType::Lanczos3);

    let mut canvas = RgbaImage::new(CANVAS, CANVAS);
    let off_x = ((CANVAS - dst_w) / 2) as i64;
    let off_y = (floor - dst_h) as i64;
    imageops::overlay(&mut canvas, &resized, off_x, off_y);

    // Re-measure on the canvas rather than transforming the old numbers: the
    // resize resamples alpha, so the feet can move by a pixel.
    let (_, _, _, _, canvas_anchor) = measure(&canvas)?;
    let _ = anchor;

    Ok((canvas, canvas_anchor))
}

/// Breathing frames from a single still.
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
            let off_y = anchor.baseline_y as i64 - nh as i64
                + (h as i64 - anchor.baseline_y as i64).min(0);
            imageops::overlay(&mut out, &scaled, off_x, off_y.max(0));
            out
        })
        .collect()
}

/// Write a pack directory: manifest, sprites, LICENSE.
pub fn write_pack(
    root: &Path,
    id: &str,
    name: &str,
    frames: &[(String, RgbaImage)],
    anchor: &Anchor,
) -> Result<PathBuf> {
    let dir = root.join(id);
    let sprites = dir.join("sprites");
    std::fs::create_dir_all(&sprites)
        .with_context(|| format!("creating {}", sprites.display()))?;

    for (file, img) in frames {
        img.save(sprites.join(file))
            .with_context(|| format!("writing {file}"))?;
    }

    let idle: Vec<&str> = frames
        .iter()
        .map(|(f, _)| f.as_str())
        .filter(|f| f.starts_with("idle_"))
        .collect();
    let still = frames
        .first()
        .map(|(f, _)| f.as_str())
        .unwrap_or("idle_00.png");

    let manifest = serde_json::json!({
        "format": 0,
        "id": id,
        "name": name,
        "author": "imported",
        "license": "All rights reserved by the importer",
        "canvas": { "width": CANVAS, "height": CANVAS },
        "anchor": { "baselineY": anchor.baseline_y, "centroidX": anchor.centroid_x },
        "states": {
            "idle":   { "frames": idle, "fps": 6, "loop": true },
            "sleep":  { "frames": [still], "fps": 1, "loop": true },
            "poke":   { "frames": [still], "fps": 10, "loop": false, "next": "idle" },
            "notify": { "frames": [still], "fps": 6, "loop": false, "next": "idle" }
        },
        "transitions": {
            "idle":  [{ "to": "sleep", "weight": 1, "minBoredom": 0.85 }],
            "sleep": [{ "to": "idle", "weight": 1, "maxBoredom": 0.2 }]
        },
        "gaze": { "enabled": false, "eyes": [], "maxOffset": 0 },
        "shadow": { "enabled": true, "opacity": 0.22, "widthRatio": 0.6, "blur": 8 },
        "sourceMap": { "default": "notify" }
    });

    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    // The spec requires a LICENSE. An imported photo belongs to whoever
    // imported it; say so rather than inventing a licence for their picture.
    std::fs::write(
        dir.join("LICENSE"),
        "This pack was generated from an image supplied by the user of this \
         machine.\nIt carries whatever rights that image already carried. It is \
         not redistributable\nby default — do not publish it without checking.\n",
    )?;

    Ok(dir)
}
