//! Windows 11 implementation.
//!
//! Status: compiles and runs, but treat it as the "keep the seam honest" side
//! until it has been driven on real hardware. Development is macOS-first; this
//! exists from day one so that macOS assumptions cannot quietly bake into the
//! architecture.
//!
//! The key flags:
//!   WS_EX_NOACTIVATE  - clicking the pet must not steal keyboard focus
//!   WS_EX_TOOLWINDOW  - no taskbar button, no Alt-Tab entry
//!   WS_EX_LAYERED     - required for per-pixel alpha
//!   WS_EX_TRANSPARENT - toggled by the hitmask loop for click-through

use anyhow::{Context, Result};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, IsWindowVisible, SetWindowLongPtrW,
    SetWindowPos, GWL_EXSTYLE, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "DeskPet";

pub fn is_start_at_login_enabled() -> Result<bool> {
    let status = Command::new("reg")
        .args(["query", RUN_KEY, "/v", RUN_VALUE])
        .status()
        .context("querying Windows login startup")?;
    Ok(status.success())
}

pub fn set_start_at_login(enabled: bool) -> Result<()> {
    let status = if enabled {
        let executable = std::env::current_exe().context("locating DeskPet executable")?;
        let command = format!("\"{}\"", executable.display());
        Command::new("reg")
            .args(["add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d"])
            .arg(command)
            .arg("/f")
            .status()
            .context("adding Windows login startup")?
    } else {
        let status = Command::new("reg")
            .args(["delete", RUN_KEY, "/v", RUN_VALUE, "/f"])
            .status()
            .context("removing Windows login startup")?;
        // `reg delete` returns 1 when the value is already absent. Off is
        // idempotent, so an absent value is already the requested state.
        if !status.success() && !is_start_at_login_enabled()? {
            return Ok(());
        }
        status
    };
    anyhow::ensure!(status.success(), "Windows registry command failed");
    Ok(())
}

pub struct WinPetWindow {
    window: tauri::WebviewWindow,
    click_through: AtomicBool,
}

impl WinPetWindow {
    pub fn new(window: tauri::WebviewWindow) -> Result<Self> {
        Ok(Self {
            window,
            click_through: AtomicBool::new(false),
        })
    }

    fn hwnd(&self) -> Result<HWND> {
        let h = self.window.hwnd().context("no HWND handle")?;
        Ok(HWND(h.0 as _))
    }
}

impl super::PetWindow for WinPetWindow {
    fn configure(&self) -> Result<()> {
        let hwnd = self.hwnd()?;
        unsafe {
            let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let next = current | WS_EX_LAYERED.0 | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next as isize);

            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )?;
        }
        Ok(())
    }

    fn set_click_through(&self, enabled: bool) -> Result<()> {
        if self.click_through.swap(enabled, Ordering::Relaxed) == enabled {
            return Ok(());
        }
        let hwnd = self.hwnd()?;
        unsafe {
            let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let next = if enabled {
                current | WS_EX_TRANSPARENT.0
            } else {
                current & !WS_EX_TRANSPARENT.0
            };
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next as isize);
        }
        Ok(())
    }

    fn set_stack_level(&self, level: super::StackLevel) -> Result<()> {
        let hwnd = self.hwnd()?;
        let insert_after = match level {
            super::StackLevel::Normal => HWND_NOTOPMOST,
            super::StackLevel::Floating => HWND_TOPMOST,
        };
        unsafe {
            SetWindowPos(
                hwnd,
                insert_after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )?;
        }
        Ok(())
    }

    fn is_fullscreen_app_active(&self) -> bool {
        unsafe {
            // Fast path: the shell already tracks this for us.
            let state = SHQueryUserNotificationState().unwrap_or(QUNS_BUSY);
            if state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE {
                return true;
            }

            // Fallback for borderless-fullscreen apps the shell misses:
            // compare the foreground window rect to its monitor rect.
            let fg = GetForegroundWindow();
            if fg.0.is_null() {
                return false;
            }
            let mut wr = RECT::default();
            if GetWindowRect(fg, &mut wr).is_err() {
                return false;
            }
            let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(mon, &mut mi).as_bool() {
                return false;
            }
            let m = mi.rcMonitor;
            wr.left <= m.left && wr.top <= m.top && wr.right >= m.right && wr.bottom >= m.bottom
        }
    }

    fn is_dnd_active(&self) -> bool {
        unsafe {
            SHQueryUserNotificationState()
                .map(|s| s == QUNS_BUSY)
                .unwrap_or(false)
        }
    }

    fn mouse_button_down(&self) -> bool {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
        // High bit set means the key is currently down.
        unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 }
    }

    fn debug_report(&self) -> String {
        let Ok(hwnd) = self.hwnd() else {
            return "no HWND handle".into();
        };
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let mut r = RECT::default();
            let ok = GetWindowRect(hwnd, &mut r).is_ok();
            format!(
                "hwnd visible={} exStyle=0x{ex:x} layered={} noactivate={} toolwindow={} \
transparent={} rect=({},{} {}x{})",
                IsWindowVisible(hwnd).as_bool(),
                ex & WS_EX_LAYERED.0 != 0,
                ex & WS_EX_NOACTIVATE.0 != 0,
                ex & WS_EX_TOOLWINDOW.0 != 0,
                ex & WS_EX_TRANSPARENT.0 != 0,
                if ok { r.left } else { 0 },
                if ok { r.top } else { 0 },
                if ok { r.right - r.left } else { 0 },
                if ok { r.bottom - r.top } else { 0 },
            )
        }
    }

    fn scale_factor(&self) -> f64 {
        // Per-Monitor V2 is declared in the manifest, so Tauri reports the
        // correct value for whichever monitor the pet currently sits on.
        self.window.scale_factor().unwrap_or(1.0)
    }
}

unsafe impl Send for WinPetWindow {}
unsafe impl Sync for WinPetWindow {}

// Silence unused import warnings in stub builds.
const _: Option<(WPARAM, LPARAM)> = None;
