//! Workaround for `winit` 0.30.x's mixed-DPI `WM_DPICHANGED` bug on Windows 11
//! (`rust-windowing/winit#4041`).
//!
//! When a restored window crosses between monitors with different display scales,
//! `winit` 0.30.13 ignores the size of the `RECT` Windows suggests in `lParam`,
//! rescales the current size a second time, and nudges the window back toward the
//! previous monitor—triggering a `WM_DPICHANGED` ping-pong that inflates the window
//! without bound.
//!
//! This Win32 subclass lets `winit` update its scale factor and notify Slint, blocks
//! re-entrant bounce-back `WM_DPICHANGED` messages, and restores the exact `RECT`
//! Windows suggested (preserving the window's logical size). Remove once Slint moves
//! to `winit` >= 0.31.

#[cfg(windows)]
mod imp {
    use std::cell::Cell;
    use std::ptr;

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, IsIconic, IsZoomed, SetWindowPos, GWL_STYLE, SWP_NOACTIVATE, SWP_NOZORDER,
        WM_DPICHANGED, WM_NCDESTROY, WS_THICKFRAME,
    };

    const SUBCLASS_ID: usize = 0x4859_5041; // "HYPA"

    thread_local! {
        static IN_DPI_CHANGE: Cell<bool> = const { Cell::new(false) };
    }

    struct ReentrancyGuard;

    impl Drop for ReentrancyGuard {
        fn drop(&mut self) {
            IN_DPI_CHANGE.with(|flag| flag.set(false));
        }
    }

    /// Installs the `WM_DPICHANGED` subclass on `window`. Returns `true` if installed.
    pub fn try_install(window: &slint::Window) -> bool {
        let handle = window.window_handle();
        let Ok(raw) = handle.window_handle() else {
            return false;
        };
        let RawWindowHandle::Win32(win32) = raw.as_raw() else {
            return false;
        };
        let hwnd = win32.hwnd.get() as HWND;
        // SAFETY: `hwnd` is a live top-level window on the UI thread and `dpi_subclass_proc`
        // matches `SUBCLASSPROC`. It removes itself on `WM_NCDESTROY`.
        let ok = unsafe { SetWindowSubclass(hwnd, Some(dpi_subclass_proc), SUBCLASS_ID, 0) };
        ok != 0
    }

    unsafe extern "system" fn dpi_subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        uid_subclass: usize,
        _ref_data: usize,
    ) -> LRESULT {
        match msg {
            WM_DPICHANGED => {
                // Block any re-entrant WM_DPICHANGED triggered by winit's own SetWindowPos
                // bouncing across the monitor boundary.
                let already_in = IN_DPI_CHANGE.with(|flag| flag.replace(true));
                if already_in {
                    return 0;
                }
                let _guard = ReentrancyGuard;

                // SAFETY: Windows passes a pointer to the suggested window RECT in `lparam`.
                let suggested = unsafe { (lparam as *const RECT).as_ref() }.copied();
                let restored_resizable = unsafe {
                    IsZoomed(hwnd) == 0
                        && IsIconic(hwnd) == 0
                        && (GetWindowLongW(hwnd, GWL_STYLE) & WS_THICKFRAME as i32) != 0
                };

                // Let winit update its scale factor and emit ScaleFactorChanged to Slint.
                let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };

                // Override winit's mis-scaled rect with the exact rect Windows suggested.
                if let (true, Some(r)) = (restored_resizable, suggested) {
                    unsafe {
                        SetWindowPos(
                            hwnd,
                            ptr::null_mut(),
                            r.left,
                            r.top,
                            r.right - r.left,
                            r.bottom - r.top,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
                result
            }
            WM_NCDESTROY => unsafe {
                RemoveWindowSubclass(hwnd, Some(dpi_subclass_proc), uid_subclass);
                DefSubclassProc(hwnd, msg, wparam, lparam)
            },
            _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
        }
    }
}

/// Installs the Windows mixed-DPI resize fix on `window` once its native handle exists.
/// Returns `true` when installed (or on non-Windows platforms).
#[cfg(windows)]
pub fn install(window: &slint::Window) -> bool {
    imp::try_install(window)
}

#[cfg(not(windows))]
pub fn install(_window: &slint::Window) -> bool {
    true
}
