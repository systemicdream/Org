//! A stronger blur behind the translucent window on macOS.
//!
//! GPUI blurs behind a `Blurred` window with an `NSVisualEffectView`, whose blur is light enough
//! that wallpaper detail shows through the sidebar. Instead, Aster hides that view and asks the
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
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGSMainConnectionID() -> *mut std::ffi::c_void;
    fn CGSSetWindowBackgroundBlurRadius(
        connection: *mut std::ffi::c_void,
        window: isize,
        radius: i64,
    ) -> i32;
}

/// Match every window's blur to `blurred` (whether the window background is translucent).
#[cfg(target_os = "macos")]
pub fn sync(blurred: bool) {
    use objc2::MainThreadMarker;
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};
    use objc2_app_kit::NSApplication;

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
            CGSSetWindowBackgroundBlurRadius(
                CGSMainConnectionID(),
                number,
                if blurred { RADIUS } else { 0 },
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn sync(_blurred: bool) {}
