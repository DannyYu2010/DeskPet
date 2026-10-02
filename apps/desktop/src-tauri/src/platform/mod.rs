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
    #[allow(dead_code)]
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

/// Give the app a full-resolution icon where the OS wants one set at runtime.
///
/// macOS needs this because `tauri dev` has no bundle to read an icon from.
/// Windows takes its icon from the executable's resources, so there is nothing
/// to do there — the no-op is the correct implementation, not a stub.
pub fn set_app_icon(png: &[u8]) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::set_dock_icon(png)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = png;
        Ok(())
    }
}

/// Run without a Dock / taskbar presence.
///
/// On Windows the equivalent is `skipTaskbar` on the window, which is already
/// set in tauri.conf.json, so there is nothing to do at runtime.
pub fn hide_from_dock() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::use_accessory_policy()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

/// Bring the app forward so a window the user just asked for is actually
/// usable. Only needed where hiding from the Dock also stops the app being
/// activated by a click.
pub fn activate_app() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::activate()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

/// Read the operating system's actual login-startup registration.
pub fn is_start_at_login_enabled() -> Result<bool> {
    #[cfg(target_os = "macos")]
    {
        macos::is_start_at_login_enabled()
    }
    #[cfg(target_os = "windows")]
    {
        windows::is_start_at_login_enabled()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(false)
    }
}

/// Add or remove DeskPet from the current user's login startup list.
pub fn set_start_at_login(enabled: bool) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::set_start_at_login(enabled)
    }
    #[cfg(target_os = "windows")]
    {
        windows::set_start_at_login(enabled)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = enabled;
        Ok(())
    }
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
