//! The app icon at runtime: the Dock icon on macOS when not running from an `.app` bundle, the
//! Wayland app id that links windows to the desktop entry (and its icon) on Linux, and the
//! image served to the UI.

use std::borrow::Cow;

use gpui::{AssetSource, SharedString};

/// The app id: the desktop entry's name on Linux, the bundle identifier on macOS.
pub const APP_ID: &str = "io.github.systemicdream.Aster";

/// The icon for the UI (welcome screen), as an embedded asset path.
pub const ICON_ASSET: &str = "app/aster-256.png";
const ICON_256: &[u8] = include_bytes!("../assets/icon/aster-256.png");

/// GPUI assets: Aster's icon, plus the UI icon set.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if path == ICON_ASSET {
            return Ok(Some(Cow::Borrowed(ICON_256)));
        }
        aster_ui::icons::Assets.load(path)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut list = aster_ui::icons::Assets.list(path)?;
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

    const ICON: &[u8] = include_bytes!("../assets/icon/aster-macos-512.png");
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
