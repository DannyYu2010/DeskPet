//! Per-pixel hit testing.
//!
//! The window is a rectangle; the pet is not. Without this, the empty corners
//! of the window swallow clicks meant for desktop icons behind it.
//!
//! Design notes:
//!   - Masks are downsampled (default 1/4) and stored as a bitset. A 512x512
//!     sprite costs 2 KB, so a whole pack's worth stays in cache.
//!   - Masks are precomputed at pack load, never per frame. Calling
//!     `getImageData` on every mouse move is the obvious approach and it is
//!     far too slow.
//!   - The frontend never touches this. It reports which frame is showing;
//!     the poll loop does the rest.

use std::collections::HashMap;

pub const DEFAULT_DOWNSCALE: u32 = 4;
/// Alpha at or below this counts as empty. Anti-aliased edges sit well above.
pub const ALPHA_THRESHOLD: u8 = 24;

#[derive(Debug, Clone)]
pub struct HitMask {
    width: u32,
    height: u32,
    downscale: u32,
    bits: Vec<u64>,
    /// Tight alpha bounds in source pixels: left, top, exclusive right/bottom.
    bounds: Option<(u32, u32, u32, u32)>,
}

impl HitMask {
    /// Build from tightly-packed RGBA8 pixels.
    pub fn from_rgba(rgba: &[u8], width: u32, height: u32, downscale: u32) -> Self {
        let downscale = downscale.max(1);
        let mw = width.div_ceil(downscale);
        let mh = height.div_ceil(downscale);
        let mut bits = vec![0u64; ((mw * mh) as usize).div_ceil(64)];
        let mut bounds: Option<(u32, u32, u32, u32)> = None;

        for y in 0..height {
            for x in 0..width {
                let alpha = rgba[((y * width + x) * 4 + 3) as usize];
                if alpha > ALPHA_THRESHOLD {
                    bounds = Some(match bounds {
                        Some((left, top, right, bottom)) => {
                            (left.min(x), top.min(y), right.max(x + 1), bottom.max(y + 1))
                        }
                        None => (x, y, x + 1, y + 1),
                    });
                }
            }
        }

        for my in 0..mh {
            for mx in 0..mw {
                // A cell is solid if ANY source pixel in it is solid. Erring
                // toward solid means edges stay clickable; the opposite would
                // make thin features like tails unclickable.
                let mut solid = false;
                'cell: for dy in 0..downscale {
                    let y = my * downscale + dy;
                    if y >= height {
                        break;
                    }
                    for dx in 0..downscale {
                        let x = mx * downscale + dx;
                        if x >= width {
                            break;
                        }
                        let alpha = rgba[((y * width + x) * 4 + 3) as usize];
                        if alpha > ALPHA_THRESHOLD {
                            solid = true;
                            break 'cell;
                        }
                    }
                }
                if solid {
                    let idx = (my * mw + mx) as usize;
                    bits[idx / 64] |= 1u64 << (idx % 64);
                }
            }
        }

        Self {
            width,
            height,
            downscale,
            bits,
            bounds,
        }
    }

    /// Test a point in window-local logical pixels.
    #[inline]
    pub fn hit(&self, x: f64, y: f64) -> bool {
        if x < 0.0 || y < 0.0 || x >= self.width as f64 || y >= self.height as f64 {
            return false;
        }
        let mw = self.width.div_ceil(self.downscale);
        let mx = (x as u32) / self.downscale;
        let my = (y as u32) / self.downscale;
        let idx = (my * mw + mx) as usize;
        self.bits
            .get(idx / 64)
            .is_some_and(|w| w & (1u64 << (idx % 64)) != 0)
    }

    pub fn bytes(&self) -> usize {
        self.bits.len() * 8
    }

    /// Whether a point is inside the tightest rectangle containing this
    /// frame's visible pixels. Used for gaze/tilt: transparent padding must
    /// not make the pet react, while gaps inside the silhouette remain part
    /// of the character's natural width and height.
    #[inline]
    pub fn within_bounds(&self, x: f64, y: f64) -> bool {
        self.bounds.is_some_and(|(left, top, right, bottom)| {
            x >= left as f64 && y >= top as f64 && x < right as f64 && y < bottom as f64
        })
    }
}

/// All masks for a loaded pack, keyed by frame id from the manifest.
#[derive(Default)]
pub struct MaskSet {
    masks: HashMap<String, HitMask>,
}

impl MaskSet {
    pub fn insert(&mut self, frame_id: impl Into<String>, mask: HitMask) {
        self.masks.insert(frame_id.into(), mask);
    }

    pub fn hit(&self, frame_id: &str, x: f64, y: f64) -> bool {
        self.masks.get(frame_id).is_some_and(|m| m.hit(x, y))
    }

    pub fn within_bounds(&self, frame_id: &str, x: f64, y: f64) -> bool {
        self.masks
            .get(frame_id)
            .is_some_and(|m| m.within_bounds(x, y))
    }

    pub fn total_bytes(&self) -> usize {
        self.masks.values().map(|m| m.bytes()).sum()
    }

    /// No pack loaded yet. The hit-test loop leaves the window interactive in
    /// this state rather than making it click-through, so that a pet without
    /// masks is still clickable instead of silently inert.
    pub fn is_empty(&self) -> bool {
        self.masks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_square(w: u32, h: u32, sx: u32, sy: u32, sw: u32, sh: u32) -> Vec<u8> {
        let mut buf = vec![0u8; (w * h * 4) as usize];
        for y in sy..(sy + sh) {
            for x in sx..(sx + sw) {
                buf[((y * w + x) * 4 + 3) as usize] = 255;
            }
        }
        buf
    }

    #[test]
    fn hits_inside_misses_outside() {
        let rgba = solid_square(64, 64, 16, 16, 32, 32);
        let m = HitMask::from_rgba(&rgba, 64, 64, 4);
        assert!(m.hit(32.0, 32.0), "center should be solid");
        assert!(!m.hit(2.0, 2.0), "corner should be transparent");
        assert!(!m.hit(-1.0, 10.0), "out of bounds is a miss");
        assert!(!m.hit(64.0, 10.0), "out of bounds is a miss");
    }

    #[test]
    fn thin_features_survive_downsampling() {
        // A 1px vertical line: must stay clickable at 1/4 scale.
        let rgba = solid_square(64, 64, 30, 0, 1, 64);
        let m = HitMask::from_rgba(&rgba, 64, 64, 4);
        assert!(m.hit(30.0, 32.0));
    }

    #[test]
    fn tight_bounds_exclude_transparent_padding() {
        let rgba = solid_square(64, 64, 16, 20, 32, 24);
        let m = HitMask::from_rgba(&rgba, 64, 64, 4);
        assert!(m.within_bounds(16.0, 20.0));
        assert!(m.within_bounds(47.9, 43.9));
        assert!(!m.within_bounds(32.0, 19.9));
        assert!(!m.within_bounds(48.0, 32.0));
    }

    #[test]
    fn mask_is_small() {
        let rgba = solid_square(512, 512, 0, 0, 512, 512);
        let m = HitMask::from_rgba(&rgba, 512, 512, 4);
        assert!(m.bytes() <= 2048, "got {} bytes", m.bytes());
    }
}
