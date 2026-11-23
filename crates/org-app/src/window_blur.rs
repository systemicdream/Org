//! A stronger blur behind the translucent window on macOS.
//!
//! GPUI blurs behind a `Blurred` window with an `NSVisualEffectView`, whose blur is light enough
//! that wallpaper detail shows through the sidebar. Instead, Org hides that view and asks the
//! window server for a stronger blur directly (the same private call Terminal.app and iTerm2
//! use, and GPUI itself on macOS 11). Call [`sync`] whenever the window background changes,
//! since GPUI adds a new effect view each time translucency is turned on.
//!
//! On Linux the compositor blurs (KDE's blur effect, Hyprland's rules), and how strongly is the
//! user's desktop setting, so there's nothing to do there.

/// Blur radius behind the window, in points. GPUI's effect view is roughly 20-30.
#[cfg(target_os = "macos")]
const RADIUS: i64 = 40;

#[cfg(target_os = "macos")]
type CgsMainConnectionId = unsafe extern "C" fn() -> *mut std::ffi::c_void;
#[cfg(target_os = "macos")]
type CgsSetWindowBackgroundBlurRadius =
    unsafe extern "C" fn(*mut std::ffi::c_void, isize, i64) -> i32;

/// Look up the private CoreGraphics blur functions dynamically so a future macOS release
/// that removes them falls back to GPUI's built-in effect view instead of failing at launch.
#[cfg(target_os = "macos")]
fn cgs_blur_fns() -> Option<(CgsMainConnectionId, CgsSetWindowBackgroundBlurRadius)> {
    use std::ffi::{c_char, c_void};
    use std::sync::OnceLock;

    const RTLD_DEFAULT: *mut c_void = (-2isize) as *mut c_void;
    unsafe extern "C" {
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    static FNS: OnceLock<Option<(CgsMainConnectionId, CgsSetWindowBackgroundBlurRadius)>> =
        OnceLock::new();
    *FNS.get_or_init(|| unsafe {
        let conn = dlsym(RTLD_DEFAULT, c"CGSMainConnectionID".as_ptr());
        let set_blur = dlsym(RTLD_DEFAULT, c"CGSSetWindowBackgroundBlurRadius".as_ptr());
        if conn.is_null() || set_blur.is_null() {
            return None;
        }
        Some((
            std::mem::transmute::<*mut c_void, CgsMainConnectionId>(conn),
            std::mem::transmute::<*mut c_void, CgsSetWindowBackgroundBlurRadius>(set_blur),
        ))
    })
}

/// Match every window's blur to `blurred` (whether the window background is translucent).
#[cfg(target_os = "macos")]
pub fn sync(blurred: bool) {
    use objc2::MainThreadMarker;
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};
    use objc2_app_kit::NSApplication;

    let Some((main_connection_id, set_blur_radius)) = cgs_blur_fns() else {
        return;
    };
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // SAFETY: plain AppKit messages to live objects on the main thread; the arrays are only
    // read here, and the window and view pointers they return stay valid while we use them.
    unsafe {
        let windows: *mut AnyObject = msg_send![&*app, windows];
        let count: usize = msg_send![windows, count];
        for i in 0..count {
            let window: *mut AnyObject = msg_send![windows, objectAtIndex: i];
            let content: *mut AnyObject = msg_send![window, contentView];
            if content.is_null() {
                continue;
            }
            let views: *mut AnyObject = msg_send![content, subviews];
            let views_count: usize = msg_send![views, count];
            for j in 0..views_count {
                let view: *mut AnyObject = msg_send![views, objectAtIndex: j];
                if (*view).class().name().to_bytes() == b"BlurredView" {
                    let _: () = msg_send![view, setHidden: Bool::YES];
                }
            }
            let number: isize = msg_send![window, windowNumber];
            set_blur_radius(
                main_connection_id(),
                number,
                if blurred { RADIUS } else { 0 },
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn sync(_blurred: bool) {}
