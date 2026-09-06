//! macOS implementation.
//!
//! The critical detail: Tauri creates an `NSWindow`, but a desktop pet must be
//! an `NSPanel` with `.nonactivatingPanel` — otherwise clicking the pet steals
//! focus from whatever you were typing in. Everything else here is secondary.
//!
//! Requires `macOSPrivateApi: true` in tauri.conf.json for the transparent
//! background. That flag makes the app ineligible for the Mac App Store; we
//! self-distribute, so this is fine, but it is documented in README.

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, sel};
use objc2_app_kit::{
    NSMainMenuWindowLevel, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};

pub struct MacPetWindow {
    window: tauri::WebviewWindow,
    click_through: AtomicBool,
}

/// `NSWindowStyleMask` bit for `NSPanel`'s non-activating behaviour.
/// Not exposed as a named constant by objc2-app-kit.
const NS_NONACTIVATING_PANEL_MASK: usize = 1 << 7;

impl MacPetWindow {
    pub fn new(window: tauri::WebviewWindow) -> Result<Self> {
        Ok(Self {
            window,
            click_through: AtomicBool::new(false),
        })
    }

    fn ns_window(&self) -> Result<Retained<NSWindow>> {
        let ptr = self.window.ns_window().context("no NSWindow handle")? as *mut AnyObject;
        anyhow::ensure!(!ptr.is_null(), "null NSWindow handle");
        // SAFETY: Tauri guarantees this pointer is a live NSWindow for the
        // lifetime of the WebviewWindow.
        unsafe { Ok(Retained::retain(ptr.cast()).context("failed to retain NSWindow")?) }
    }
}

impl super::PetWindow for MacPetWindow {
    fn configure(&self) -> Result<()> {
        let win = self.ns_window()?;

        unsafe {
            // 1. Reclassify as a non-activating panel. This is what stops the
            //    pet from stealing keyboard focus on click.
            let mask = NSWindowStyleMask(
                NSWindowStyleMask::Borderless.0 | NS_NONACTIVATING_PANEL_MASK,
            );
            win.setStyleMask(mask);

            // 2. Follow the user across Spaces, and be allowed to sit on top of
            //    other apps' fullscreen windows. `.stationary` keeps the pet
            //    still during Mission Control instead of flying around.
            win.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::Stationary,
            );

            // 3. Above normal windows, below the menu bar. See StackLevel docs
            //    for why we don't go higher.
            win.setLevel(NSMainMenuWindowLevel as isize - 1);

            win.setOpaque(false);
            win.setHasShadow(false); // we draw our own contact shadow
            win.setIgnoresMouseEvents(false);

            // 4. Never take part in window cycling or state restoration.
            let _: () = msg_send![&*win, setHidesOnDeactivate: false];
            let _: () = msg_send![&*win, setRestorable: false];
        }

        Ok(())
    }

    fn set_click_through(&self, enabled: bool) -> Result<()> {
        // Called at up to 30 Hz from the hitmask loop; skip the ObjC round trip
        // when nothing changed.
        if self.click_through.swap(enabled, Ordering::Relaxed) == enabled {
            return Ok(());
        }
        let win = self.ns_window()?;
        unsafe { win.setIgnoresMouseEvents(enabled) };
        Ok(())
    }

    fn set_stack_level(&self, level: super::StackLevel) -> Result<()> {
        let win = self.ns_window()?;
        let value = match level {
            super::StackLevel::Normal => 0isize,
            super::StackLevel::Floating => NSMainMenuWindowLevel as isize - 1,
        };
        unsafe { win.setLevel(value) };
        Ok(())
    }

    fn is_fullscreen_app_active(&self) -> bool {
        // A frontmost window whose frame equals the full screen frame (with no
        // menu bar visible) means someone is in fullscreen. Checking the
        // presentation options of the frontmost app is the cheap way to do it.
        //
        // TODO(macos): implement via NSApplication.currentSystemPresentationOptions
        // Placeholder keeps behaviour identical to "no fullscreen app".
        false
    }

    fn is_dnd_active(&self) -> bool {
        // macOS Focus state is readable from
        // ~/Library/DoNotDisturb/DB/Assertions.json without any permission
        // prompt. Parse lazily and cache — this is polled at 1 Hz.
        //
        // TODO(macos): implement Assertions.json read.
        false
    }

    fn scale_factor(&self) -> f64 {
        self.window.scale_factor().unwrap_or(2.0)
    }
}

// SAFETY: all AppKit calls above are dispatched on the main thread by Tauri's
// run loop; the struct itself holds no non-Send state beyond an atomic.
unsafe impl Send for MacPetWindow {}
unsafe impl Sync for MacPetWindow {}
