//! macOS implementation.
//!
//! The critical detail: Tauri creates an `NSWindow`, but a desktop pet must be
//! an `NSPanel` with `.nonactivatingPanel` — otherwise clicking the pet steals
//! focus from whatever you were typing in. Everything else here is secondary.
//!
//! Requires `macOSPrivateApi: true` in tauri.conf.json for the transparent
//! background. That flag makes the app ineligible for the Mac App Store; we
//! self-distribute, so this is fine, but it is documented in README.
//!
//! ## Why the class swap
//!
//! `NSWindowStyleMaskNonactivatingPanel` (bit 7) is `NSPanel`-only. Setting it
//! on a plain `NSWindow` raises `NSInternalInconsistencyException` and kills
//! the process — that was crash 1, diagnosed and reproducible. The instance
//! therefore has to *become* an `NSPanel` before the mask is applied, which on
//! macOS means `object_setClass`. `NSPanel` declares no instance variables of
//! its own beyond `NSWindow`'s, so the instance layout is unchanged and the
//! swap is safe. This is the same mechanism `tauri-nspanel` uses.
//!
//! Every step below prints to stderr before it runs. If the process dies, the
//! last line printed names the call that killed it — the alternative is
//! guessing from a crash report.

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use tauri::Manager;
use objc2_app_kit::{
    NSMainMenuWindowLevel, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};

extern "C" {
    /// Declared here rather than taken from `objc2::ffi` so that an objc2
    /// point release cannot silently change the signature under us.
    fn object_setClass(obj: *mut AnyObject, cls: *const AnyClass) -> *mut AnyObject;
}

/// `NSWindowStyleMask` bit for `NSPanel`'s non-activating behaviour.
/// Not exposed as a named constant by objc2-app-kit.
const NS_NONACTIVATING_PANEL_MASK: usize = 1 << 7;

macro_rules! step {
    ($($arg:tt)*) => {
        eprintln!("[deskpet/macos] {}", format_args!($($arg)*));
    };
}

pub struct MacPetWindow {
    window: tauri::WebviewWindow,
    click_through: AtomicBool,
}

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

    /// Run `f` against the `NSWindow` on the main thread.
    ///
    /// Both callers below are driven from worker threads — the hitmask loop at
    /// 30 Hz and the environment loop at 1 Hz — and **every** AppKit window
    /// mutation must happen on the main thread. Calling `setIgnoresMouseEvents`
    /// straight from the worker is the kind of bug that looks fine for a while
    /// and then kills the process at a random moment.
    ///
    /// The pointer is passed across as a `usize` because the closure has to be
    /// `Send`. That is sound here: the value is only dereferenced inside the
    /// closure, which by construction runs on the main thread, and the window
    /// outlives the app.
    fn on_main<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&NSWindow) + Send + 'static,
    {
        let ptr = self.window.ns_window().context("no NSWindow handle")? as usize;
        anyhow::ensure!(ptr != 0, "null NSWindow handle");
        self.window
            .app_handle()
            .run_on_main_thread(move || {
                // SAFETY: see the doc comment. Main thread, live window.
                let win: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
                f(win);
            })
            .context("failed to dispatch to the main thread")
    }

    /// Reclassify the instance as `NSPanel`. Idempotent: a window that already
    /// answers to `NSPanel` is left alone.
    ///
    /// SAFETY: caller must be on the main thread and the window must not yet
    /// be on screen.
    unsafe fn become_panel(&self, win: &NSWindow) -> Result<()> {
        let obj = win as *const NSWindow as *mut AnyObject;

        let panel_cls = AnyClass::get(c"NSPanel").context("NSPanel class not registered")?;

        let already: bool = msg_send![&*obj, isKindOfClass: panel_cls];
        if already {
            step!("already an NSPanel, skipping class swap");
            return Ok(());
        }

        let current: *const AnyClass = msg_send![&*obj, class];
        step!(
            "class swap: {} -> NSPanel",
            (*current).name().to_str().unwrap_or("<unknown>")
        );

        object_setClass(obj, panel_cls as *const AnyClass);

        let now: bool = msg_send![&*obj, isKindOfClass: panel_cls];
        anyhow::ensure!(now, "object_setClass did not take effect");
        Ok(())
    }
}

impl super::PetWindow for MacPetWindow {
    fn configure(&self) -> Result<()> {
        // AppKit window mutation is main-thread only. Tauri's setup hook runs
        // on the main thread, but assert it rather than trust it — suspect 2
        // in the handoff brief.
        let main_thread: bool = unsafe {
            let cls = AnyClass::get(c"NSThread").context("NSThread class not registered")?;
            msg_send![cls, isMainThread]
        };
        step!("configure() entered, main thread = {main_thread}");
        anyhow::ensure!(
            main_thread,
            "configure() must run on the main thread; it was called from a worker"
        );

        let win = self.ns_window()?;
        step!("got NSWindow handle");

        unsafe {
            // 1. Become a panel first. The non-activating mask below is
            //    NSPanel-only and raises on a plain NSWindow.
            self.become_panel(&win)?;

            step!("setStyleMask(borderless | nonactivatingPanel)");
            let mask = NSWindowStyleMask(
                NSWindowStyleMask::Borderless.0 | NS_NONACTIVATING_PANEL_MASK,
            );
            win.setStyleMask(mask);

            // 2. Panel behaviour. `becomesKeyOnlyIfNeeded` is the one that
            //    makes clicking the pet a no-op for keyboard focus: the panel
            //    only takes key status if something in it actually wants
            //    keystrokes, which for a pet is never.
            step!("setFloatingPanel / setBecomesKeyOnlyIfNeeded / setWorksWhenModal");
            let _: () = msg_send![&*win, setFloatingPanel: true];
            let _: () = msg_send![&*win, setBecomesKeyOnlyIfNeeded: true];
            let _: () = msg_send![&*win, setWorksWhenModal: true];

            // 3. Follow the user across Spaces, and be allowed to sit on top of
            //    other apps' fullscreen windows. `.stationary` keeps the pet
            //    still during Mission Control instead of flying around.
            step!("setCollectionBehavior");
            win.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::Stationary,
            );

            // 4. Above normal windows, below the menu bar. See StackLevel docs
            //    for why we don't go higher.
            step!("setLevel");
            win.setLevel(NSMainMenuWindowLevel as isize - 1);

            step!("setOpaque / setHasShadow / setIgnoresMouseEvents");
            win.setOpaque(false);
            win.setHasShadow(false); // we draw our own contact shadow
            win.setIgnoresMouseEvents(false);

            // 5. Never take part in window cycling or state restoration.
            step!("setHidesOnDeactivate / setRestorable / setMovableByWindowBackground");
            let _: () = msg_send![&*win, setHidesOnDeactivate: false];
            let _: () = msg_send![&*win, setRestorable: false];
            // Drag the pet by grabbing anywhere on it. The hitmask loop already
            // makes empty pixels click-through, so this only affects the pet.
            let _: () = msg_send![&*win, setMovableByWindowBackground: true];
        }

        step!("configure() completed");
        Ok(())
    }

    fn set_click_through(&self, enabled: bool) -> Result<()> {
        // Called at up to 30 Hz from the hitmask loop; skip the ObjC round trip
        // when nothing changed.
        if self.click_through.swap(enabled, Ordering::Relaxed) == enabled {
            return Ok(());
        }
        self.on_main(move |win| win.setIgnoresMouseEvents(enabled))
    }

    fn set_stack_level(&self, level: super::StackLevel) -> Result<()> {
        let value = match level {
            super::StackLevel::Normal => 0isize,
            super::StackLevel::Floating => NSMainMenuWindowLevel as isize - 1,
        };
        self.on_main(move |win| win.setLevel(value))
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

    fn mouse_button_down(&self) -> bool {
        // NSEvent.pressedMouseButtons is a class method and does not touch the
        // window, so it is safe to call from the hitmask worker.
        unsafe {
            let Some(cls) = AnyClass::get(c"NSEvent") else {
                return false;
            };
            let buttons: usize = msg_send![cls, pressedMouseButtons];
            buttons != 0
        }
    }

    fn debug_report(&self) -> String {
        let Ok(win) = self.ns_window() else {
            return "no NSWindow handle".into();
        };
        unsafe {
            let obj = &*win as *const NSWindow as *mut AnyObject;
            let cls: *const AnyClass = msg_send![&*obj, class];
            let class_name = (*cls).name().to_str().unwrap_or("<unknown>").to_string();

            let visible: bool = msg_send![&*obj, isVisible];
            let on_screen: bool = msg_send![&*obj, isOnActiveSpace];
            let key: bool = msg_send![&*obj, isKeyWindow];
            let floating: bool = msg_send![&*obj, isFloatingPanel];
            let opaque: bool = msg_send![&*obj, isOpaque];
            let alpha: f64 = msg_send![&*obj, alphaValue];
            let level: isize = msg_send![&*obj, level];
            let mask: usize = msg_send![&*obj, styleMask];
            let frame = win.frame();

            // Which display the pet is on, and how the displays are laid out.
            // Without this, "it went to the other monitor" and "the Dock moved"
            // are both unfalsifiable from a log.
            let screens: *mut AnyObject = {
                let cls = AnyClass::get(c"NSScreen");
                match cls {
                    Some(c) => msg_send![c, screens],
                    None => std::ptr::null_mut(),
                }
            };
            let mut layout = String::new();
            if !screens.is_null() {
                let count: usize = msg_send![&*screens, count];
                for i in 0..count {
                    let scr: *mut AnyObject = msg_send![&*screens, objectAtIndex: i];
                    let f: objc2_foundation::NSRect = msg_send![&*scr, frame];
                    layout.push_str(&format!(
                        " screen{i}=({},{} {}x{})",
                        f.origin.x, f.origin.y, f.size.width, f.size.height
                    ));
                }
            }
            let win_screen: *mut AnyObject = msg_send![&*obj, screen];
            let on_screen_idx = if win_screen.is_null() {
                "none".to_string()
            } else {
                let f: objc2_foundation::NSRect = msg_send![&*win_screen, frame];
                format!("({},{} {}x{})", f.origin.x, f.origin.y, f.size.width, f.size.height)
            };

            format!(
                "class={class_name} visible={visible} onActiveSpace={on_screen} \
key={key} floatingPanel={floating} opaque={opaque} alpha={alpha} level={level} \
styleMask=0x{mask:x} nonactivating={} frame=({},{} {}x{}) \
onScreen={on_screen_idx} displays:{layout}",
                mask & NS_NONACTIVATING_PANEL_MASK != 0,
                frame.origin.x,
                frame.origin.y,
                frame.size.width,
                frame.size.height,
            )
        }
    }

    fn scale_factor(&self) -> f64 {
        self.window.scale_factor().unwrap_or(2.0)
    }
}

// SAFETY: `configure` and `debug_report` run on the main thread during setup;
// everything reachable from a worker thread goes through `on_main`. The struct
// itself holds no non-Send state beyond an atomic.
unsafe impl Send for MacPetWindow {}
unsafe impl Sync for MacPetWindow {}
