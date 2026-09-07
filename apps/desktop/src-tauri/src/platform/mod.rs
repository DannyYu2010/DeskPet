//! The ONLY place in the codebase where OS differences are allowed.
//!
//! ## Hard rule
//! `#[cfg(target_os = ...)]` must not appear anywhere outside this module,
//! and must never appear in TypeScript. If you find yourself wanting a
//! platform branch in `core/` or in the frontend, the correct fix is to add
//! a method to `PetWindow` and implement it on both sides.
//!
//! CI enforces this with a grep check (see .github/workflows/ci.yml).

use anyhow::Result;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// How aggressively the pet floats above other windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackLevel {
    /// Normal window ordering. Used when a fullscreen app is detected.
    Normal,
    /// Above regular windows, below system UI. This is the default.
    ///
    /// Deliberately NOT the highest available level: on macOS `.screenSaver`
    /// covers menu bar and system alerts, which users find hostile.
    Floating,
}

/// Capabilities that genuinely differ between macOS and Windows.
///
/// Implementations must be cheap to call: `is_fullscreen_app_active` and
/// `is_dnd_active` are polled on a low-frequency timer (~1 Hz).
pub trait PetWindow: Send + Sync {
    /// One-time setup: transparent, borderless, non-activating, no taskbar
    /// entry, visible across all desktops/spaces.
    ///
    /// Must be called before the window is first shown.
    fn configure(&self) -> Result<()>;

    /// Toggle click-through. Driven by the hitmask poll loop, so this is
    /// called at up to 30 Hz and must not allocate or block.
    fn set_click_through(&self, enabled: bool) -> Result<()>;

    fn set_stack_level(&self, level: StackLevel) -> Result<()>;

    /// True when the foreground app owns the whole screen (games, video,
    /// presentations). The pet hides itself in this state.
    fn is_fullscreen_app_active(&self) -> bool;

    /// True when the OS "do not disturb" / "focus" mode is on.
    /// The pet stays visible but suppresses all notification reactions.
    fn is_dnd_active(&self) -> bool;

    /// True while any mouse button is physically held down, anywhere on the
    /// system.
    ///
    /// The hit-test loop uses this to leave click-through alone mid-press.
    /// Cheap: one call into the window server, polled at 30 Hz.
    fn mouse_button_down(&self) -> bool;

    /// One-line description of the window's actual on-screen state, for the
    /// log. Called once after the window is shown.
    ///
    /// This exists because "the process did not crash" and "the pet is on the
    /// screen" are different claims, and the first one is much easier to
    /// mistake for the second.
    fn debug_report(&self) -> String;

    /// Logical -> physical scale for the display the pet currently sits on.
    /// Needed because the pet moves between monitors with different DPI.
    fn scale_factor(&self) -> f64;
}

/// Construct the platform implementation for a Tauri window.
pub fn attach(window: &tauri::WebviewWindow) -> Result<Box<dyn PetWindow>> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::MacPetWindow::new(window.clone())?))
    }
    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(windows::WinPetWindow::new(window.clone())?))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = window;
        anyhow::bail!("DeskPet supports macOS and Windows 11 only")
    }
}
