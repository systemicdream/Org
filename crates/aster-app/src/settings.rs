//! User settings: appearance, editor text size, keymap and capture templates. Stored as JSON in
//! the platform config dir and edited on the settings page (never by hand, though it's readable).

use std::path::PathBuf;

use aster_ui::{Appearance, ThemePrefs};
use gpui::{App, Global, WindowBackgroundAppearance};
use serde::{Deserialize, Serialize};

pub const FONT_SIZE_MIN: f32 = 12.;
pub const FONT_SIZE_MAX: f32 = 24.;
pub const FONT_SIZE_DEFAULT: f32 = 16.;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppearancePref {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Keymap {
    /// Platform conventions (Cmd/Ctrl shortcuts).
    #[default]
    Standard,
    /// Standard, plus Emacs/Org-style bindings in the editor.
    Emacs,
    /// Standard, plus modal Vim editing (Normal/Insert/Visual) in the editor.
    Vim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureKind {
    /// `* TODO text` with a creation timestamp.
    Task,
    /// `* text` with a creation timestamp.
    Note,
    /// `** HH:MM text` under today's date heading.
    Journal,
}

impl CaptureKind {
    pub const ALL: [CaptureKind; 3] = [CaptureKind::Task, CaptureKind::Note, CaptureKind::Journal];

    pub fn label(self) -> &'static str {
        match self {
            CaptureKind::Task => "Task",
            CaptureKind::Note => "Note",
            CaptureKind::Journal => "Journal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureTemplate {
    pub name: String,
    pub kind: CaptureKind,
    /// Target file, relative to the notes folder.
    pub file: String,
    /// File under this heading (created if missing); empty for the top level. Not used by
    /// journal templates, which file under today's date.
    #[serde(default)]
    pub heading: String,
}

impl CaptureTemplate {
    fn new(name: &str, kind: CaptureKind, file: &str) -> Self {
        Self {
            name: name.into(),
            kind,
            file: file.into(),
            heading: String::new(),
        }
    }

    /// The target file as a safe relative path ending in `.org`, or `None` if unusable
    /// (empty, absolute, or leaving the notes folder).
    pub fn target(&self) -> Option<PathBuf> {
        let file = self.file.trim();
        if file.is_empty() {
            return None;
        }
        let mut path = PathBuf::new();
        for part in file.split(['/', '\\']) {
            match part {
                "" | "." => {}
                ".." => return None,
                p if p.contains(':') => return None,
                p => path.push(p),
            }
        }
        if file.starts_with(['/', '\\']) || path.as_os_str().is_empty() {
            return None;
        }
        if path.extension().is_none_or(|e| e != "org") {
            let mut name = path.file_name()?.to_os_string();
            name.push(".org");
            path.set_file_name(name);
        }
        Some(path)
    }

    /// A one-line description for the capture dialog, e.g. "Adds “TODO …” to inbox.org".
    pub fn describe(&self) -> String {
        let file = self
            .target()
            .map_or("(no file)".into(), |p| p.display().to_string());
        let heading = self.heading.trim();
        let under = if heading.is_empty() || self.kind == CaptureKind::Journal {
            String::new()
        } else {
            format!(" under “{heading}”")
        };
        match self.kind {
            CaptureKind::Task => format!("Adds “TODO …” to {file}{under}"),
            CaptureKind::Note => format!("Adds a note to {file}{under}"),
            CaptureKind::Journal => format!("Adds a timestamped entry under today in {file}"),
        }
    }
}

pub fn default_templates() -> Vec<CaptureTemplate> {
    vec![
        CaptureTemplate::new("Task", CaptureKind::Task, "inbox.org"),
        CaptureTemplate::new("Note", CaptureKind::Note, "inbox.org"),
        CaptureTemplate::new("Journal", CaptureKind::Journal, "journal.org"),
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub appearance: AppearancePref,
    /// `#rrggbb`, or `None` for the system accent.
    pub accent: Option<String>,
    pub font_size: f32,
    /// Mica-style translucent window background, where the desktop supports blur.
    pub transparency: bool,
    pub keymap: Keymap,
    pub capture_templates: Vec<CaptureTemplate>,
    /// Ask GitHub Releases once a day whether a newer Aster is out.
    pub check_for_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: AppearancePref::System,
            accent: None,
            font_size: FONT_SIZE_DEFAULT,
            transparency: true,
            keymap: Keymap::Standard,
            capture_templates: default_templates(),
            check_for_updates: true,
        }
    }
}

impl Global for Settings {}

/// Whether the window can have a blurred background: macOS, and Wayland compositors known to
/// blur behind windows (KDE Plasma via its blur protocol; Hyprland by its own rules). On others
/// (e.g. GNOME) a translucent window would show the desktop unblurred, so Aster stays opaque.
pub fn transparency_supported() -> bool {
    if cfg!(target_os = "macos") {
        return true;
    }
    if cfg!(target_os = "linux") {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        return desktop
            .split(':')
            .any(|d| d.eq_ignore_ascii_case("KDE") || d.eq_ignore_ascii_case("Hyprland"));
    }
    false
}

/// `$ASTER_SETTINGS_FILE` (for tests and multiple setups) or the platform config dir.
fn settings_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("ASTER_SETTINGS_FILE") {
        return Some(PathBuf::from(p));
    }
    Some(dirs::config_dir()?.join("aster").join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        settings_path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .unwrap_or_default()
            .sanitized()
    }

    /// Clamp values a hand edit could have broken.
    fn sanitized(mut self) -> Self {
        self.font_size = if self.font_size.is_finite() {
            self.font_size.clamp(FONT_SIZE_MIN, FONT_SIZE_MAX).round()
        } else {
            FONT_SIZE_DEFAULT
        };
        if self
            .accent
            .as_deref()
            .is_some_and(|a| aster_ui::accent::parse_hex(a).is_none())
        {
            self.accent = None;
        }
        self
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let Some(path) = settings_path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        crate::workspace::write_atomic(&path, &json)
    }

    pub fn theme_prefs(&self) -> ThemePrefs {
        ThemePrefs {
            appearance: match self.appearance {
                AppearancePref::System => None,
                AppearancePref::Light => Some(Appearance::Light),
                AppearancePref::Dark => Some(Appearance::Dark),
            },
            accent: self.accent.as_deref().and_then(aster_ui::accent::parse_hex),
            translucent: self.translucent(),
        }
    }

    /// Whether the window background is translucent (the setting, where it's supported).
    pub fn translucent(&self) -> bool {
        self.transparency && transparency_supported()
    }

    pub fn window_background(&self) -> WindowBackgroundAppearance {
        if self.translucent() {
            WindowBackgroundAppearance::Blurred
        } else {
            WindowBackgroundAppearance::Opaque
        }
    }

    /// Editor line height for the font size (the type ramp's 16/26 ratio).
    pub fn line_height(&self) -> f32 {
        (self.font_size * 1.625).round()
    }
}

/// Change the settings, save them and apply them everywhere. Returns the save error, if any.
pub fn update(cx: &mut App, f: impl FnOnce(&mut Settings)) -> anyhow::Result<()> {
    let mut settings = cx.global::<Settings>().clone();
    let before = settings.clone();
    f(&mut settings);
    if settings == before {
        return Ok(());
    }
    if settings.keymap != before.keymap {
        crate::keymap::apply(settings.keymap, cx);
    }
    aster_ui::set_prefs(settings.theme_prefs(), cx);
    if settings.window_background() != before.window_background() {
        let background = settings.window_background();
        for window in cx.windows() {
            window
                .update(cx, |_, window, _| {
                    window.set_background_appearance(background)
                })
                .ok();
        }
    }
    let result = settings.save();
    cx.set_global(settings);
    cx.refresh_windows();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_targets_stay_inside_the_folder() {
        let t = |f: &str| {
            CaptureTemplate::new("x", CaptureKind::Task, f)
                .target()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
        };
        assert_eq!(t("inbox.org").as_deref(), Some("inbox.org"));
        assert_eq!(t("notes/work").as_deref(), Some("notes/work.org"));
        assert_eq!(t("./a/./b.org").as_deref(), Some("a/b.org"));
        assert_eq!(t("tasks.txt").as_deref(), Some("tasks.txt.org"));
        assert_eq!(t("../escape.org"), None);
        assert_eq!(t("/etc/passwd"), None);
        assert_eq!(t("C:/x.org"), None);
        assert_eq!(t("  "), None);
    }

    #[test]
    fn loads_partial_and_broken_settings() {
        let s: Settings = serde_json::from_str(r#"{"font_size": 99, "accent": "nope"}"#).unwrap();
        let s = s.sanitized();
        assert_eq!(s.font_size, FONT_SIZE_MAX);
        assert_eq!(s.accent, None);
        assert_eq!(s.capture_templates, default_templates());
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert_eq!(
            serde_json::from_str::<Settings>(&json).unwrap(),
            Settings::default()
        );
    }
}
