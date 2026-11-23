//! The app icon at runtime: the Dock icon on macOS when not running from an `.app` bundle, the
//! Wayland app id that links windows to the desktop entry (and its icon) on Linux, and the
//! image served to the UI.

use std::borrow::Cow;

use gpui::{AssetSource, SharedString};

/// The app id: the desktop entry's name on Linux, the bundle identifier on macOS.
pub const APP_ID: &str = "io.github.systemicdream.Org";

/// The icon for the UI (welcome screen), as an embedded asset path.
pub const ICON_ASSET: &str = "app/org-256.png";
const ICON_256: &[u8] = include_bytes!("../assets/icon/org-256.png");

/// GPUI assets: Org's icon, plus the UI icon set.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if path == ICON_ASSET {
            return Ok(Some(Cow::Borrowed(ICON_256)));
        }
        org_ui::icons::Assets.load(path)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut list = org_ui::icons::Assets.list(path)?;
        if ICON_ASSET.starts_with(path) {
            list.push(ICON_ASSET.into());
        }
        Ok(list)
    }
}

/// Give the Dock our icon when running a bare binary (a bundle gets it from its Info.plist).
#[cfg(target_os = "macos")]
pub fn set_dock_icon() {
    use objc2::{AllocAnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    const ICON: &[u8] = include_bytes!("../assets/icon/org-macos-512.png");
    let bundled =
        std::env::current_exe().is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"));
    if bundled {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(ICON);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    unsafe { app.setApplicationIconImage(Some(&image)) };
}

#[cfg(not(target_os = "macos"))]
pub fn set_dock_icon() {}

/// Name the app "Org" in the menu bar when running a bare binary. macOS takes the name from
/// the bundle's Info.plist and, without one, uses the executable's name (`org`). Call before
/// the app launches.
#[cfg(target_os = "macos")]
pub fn set_app_name() {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
    use objc2_foundation::NSString;

    let Some(bundle_class) = AnyClass::get(c"NSBundle") else {
        return;
    };
    // SAFETY: plain Foundation messages on the main thread before NSApplication starts. The
    // info dictionary of a bare binary's main bundle is mutable; check before writing to it.
    unsafe {
        let bundle: *mut AnyObject = msg_send![bundle_class, mainBundle];
        if bundle.is_null() {
            return;
        }
        let info: *mut AnyObject = msg_send![bundle, infoDictionary];
        if info.is_null() {
            return;
        }
        let set = Sel::register(c"setObject:forKey:");
        let mutable: Bool = msg_send![info, respondsToSelector: set];
        if !mutable.as_bool() {
            return;
        }
        let name = NSString::from_str("Org");
        let key = NSString::from_str("CFBundleName");
        let _: () = msg_send![info, setObject: &*name, forKey: &*key];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_app_name() {}
