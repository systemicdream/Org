//! Aster's design system: Fluent (early Windows 11) theme tokens for light and dark, the type
//! ramp, geometry, elevation, and small building blocks used across the UI.
//!
//! The values and their meaning are documented in `docs/DESIGN.md`. Views must use these
//! tokens instead of literal colors or sizes.

pub mod accent;
pub mod components;
pub mod icons;

use gpui::{App, BoxShadow, Global, Hsla, Pixels, Rgba, WindowAppearance, point, px};

/// Light or dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

impl From<WindowAppearance> for Appearance {
    fn from(a: WindowAppearance) -> Self {
        match a {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Appearance::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => Appearance::Light,
        }
    }
}

/// `#RRGGBB` with an alpha in 0..=1.
fn c(hex: u32, alpha: f32) -> Hsla {
    let mut rgba: Rgba = gpui::rgb(hex);
    rgba.a = alpha;
    rgba.into()
}

/// Color tokens (names follow WinUI theme resources; see docs/DESIGN.md §3).
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    // Text
    pub text_primary: Hsla,
    pub text_secondary: Hsla,
    pub text_tertiary: Hsla,
    pub text_disabled: Hsla,
    pub text_on_accent: Hsla,
    // Accent
    pub accent: Hsla,
    pub accent_secondary: Hsla,
    pub accent_tertiary: Hsla,
    pub accent_subtle: Hsla,
    pub selection: Hsla,
    // Surfaces
    pub base: Hsla,
    pub layer: Hsla,
    pub card: Hsla,
    pub flyout: Hsla,
    pub smoke: Hsla,
    // Fills
    pub control: Hsla,
    pub control_secondary: Hsla,
    pub control_tertiary: Hsla,
    pub control_input_active: Hsla,
    pub subtle_secondary: Hsla,
    pub subtle_tertiary: Hsla,
    // Strokes
    pub stroke_control: Hsla,
    pub stroke_control_strong: Hsla,
    pub stroke_divider: Hsla,
    pub stroke_surface: Hsla,
    pub stroke_focus: Hsla,
    // Status
    pub success: Hsla,
    pub success_bg: Hsla,
    pub caution: Hsla,
    pub caution_bg: Hsla,
    pub critical: Hsla,
    pub critical_bg: Hsla,
    pub neutral_bg: Hsla,
}

impl Colors {
    /// Replace the default accent with shades derived from `base` (e.g. the system accent).
    pub fn with_accent(mut self, base: Rgba, dark: bool) -> Self {
        let a = accent::derive(base, dark, &[self.base, self.layer, self.flyout]);
        let with_alpha = |alpha: f32| Hsla { a: alpha, ..a };
        self.accent = a;
        self.accent_secondary = with_alpha(0.9);
        self.accent_tertiary = with_alpha(0.8);
        self.accent_subtle = with_alpha(if dark { 0.14 } else { 0.10 });
        self.selection = with_alpha(if dark { 0.30 } else { 0.25 });
        self.text_on_accent = accent::on_accent(a);
        self
    }

    pub fn light() -> Self {
        Self {
            text_primary: c(0x1A1A1A, 1.),
            text_secondary: c(0x5D5D5D, 1.),
            text_tertiary: c(0x8A8A8A, 1.),
            text_disabled: c(0xA0A0A0, 1.),
            text_on_accent: c(0xFFFFFF, 1.),
            accent: c(0x005FB8, 1.),
            accent_secondary: c(0x005FB8, 0.9),
            accent_tertiary: c(0x005FB8, 0.8),
            accent_subtle: c(0x005FB8, 0.10),
            selection: c(0x005FB8, 0.25),
            base: c(0xF3F3F3, 1.),
            layer: c(0xFBFBFB, 1.),
            card: c(0xFFFFFF, 0.7),
            flyout: c(0xFCFCFC, 1.),
            smoke: c(0x000000, 0.3),
            control: c(0xFFFFFF, 0.7),
            control_secondary: c(0xF9F9F9, 0.5),
            control_tertiary: c(0xF9F9F9, 0.3),
            control_input_active: c(0xFFFFFF, 1.),
            subtle_secondary: c(0x000000, 0.037),
            subtle_tertiary: c(0x000000, 0.024),
            stroke_control: c(0x000000, 0.058),
            stroke_control_strong: c(0x000000, 0.16),
            stroke_divider: c(0x000000, 0.08),
            stroke_surface: c(0x000000, 0.058),
            stroke_focus: c(0x000000, 0.9),
            success: c(0x0F7B0F, 1.),
            success_bg: c(0xDFF6DD, 1.),
            caution: c(0x9D5D00, 1.),
            caution_bg: c(0xFFF4CE, 1.),
            critical: c(0xC42B1C, 1.),
            critical_bg: c(0xFDE7E9, 1.),
            neutral_bg: c(0x000000, 0.04),
        }
    }

    pub fn dark() -> Self {
        Self {
            text_primary: c(0xFFFFFF, 1.),
            text_secondary: c(0xC5C5C5, 1.),
            text_tertiary: c(0x9E9E9E, 1.),
            text_disabled: c(0x717171, 1.),
            text_on_accent: c(0x000000, 1.),
            accent: c(0x60CDFF, 1.),
            accent_secondary: c(0x60CDFF, 0.9),
            accent_tertiary: c(0x60CDFF, 0.8),
            accent_subtle: c(0x60CDFF, 0.14),
            selection: c(0x60CDFF, 0.30),
            base: c(0x202020, 1.),
            layer: c(0x282828, 1.),
            card: c(0xFFFFFF, 0.05),
            flyout: c(0x2C2C2C, 1.),
            smoke: c(0x000000, 0.3),
            control: c(0xFFFFFF, 0.06),
            control_secondary: c(0xFFFFFF, 0.08),
            control_tertiary: c(0xFFFFFF, 0.04),
            control_input_active: c(0x1E1E1E, 1.),
            subtle_secondary: c(0xFFFFFF, 0.06),
            subtle_tertiary: c(0xFFFFFF, 0.04),
            stroke_control: c(0xFFFFFF, 0.07),
            stroke_control_strong: c(0xFFFFFF, 0.09),
            stroke_divider: c(0xFFFFFF, 0.08),
            stroke_surface: c(0x000000, 0.2),
            stroke_focus: c(0xFFFFFF, 1.),
            success: c(0x6CCB5F, 1.),
            success_bg: c(0x393D1B, 1.),
            caution: c(0xFCE100, 1.),
            caution_bg: c(0x433519, 1.),
            critical: c(0xFF99A4, 1.),
            critical_bg: c(0x442726, 1.),
            neutral_bg: c(0xFFFFFF, 0.06),
        }
    }
}

/// A text style from the Fluent type ramp: (font size, line height).
#[derive(Debug, Clone, Copy)]
pub struct TextStyle {
    pub size: Pixels,
    pub line_height: Pixels,
}

const fn ts(size: f32, line_height: f32) -> TextStyle {
    TextStyle {
        size: px(size),
        line_height: px(line_height),
    }
}

/// The type ramp (docs/DESIGN.md §4). Weights are applied by the view (Semibold for Strong,
/// Subtitle and Title styles).
pub mod ramp {
    use super::{TextStyle, ts};
    pub const CAPTION: TextStyle = ts(12., 16.);
    pub const BODY: TextStyle = ts(14., 20.);
    pub const BODY_LARGE: TextStyle = ts(18., 24.);
    pub const SUBTITLE: TextStyle = ts(20., 28.);
    pub const TITLE: TextStyle = ts(28., 36.);
    pub const TITLE_LARGE: TextStyle = ts(40., 52.);
    /// Note text in the editor.
    pub const EDITOR: TextStyle = ts(16., 26.);
}

/// Corner radii (docs/DESIGN.md §5).
pub const RADIUS_CONTROL: Pixels = px(4.);
pub const RADIUS_OVERLAY: Pixels = px(8.);

/// Elevation levels (docs/DESIGN.md §6).
#[derive(Debug, Clone, Copy)]
pub enum Elevation {
    Card,
    Tooltip,
    Flyout,
    Dialog,
}

/// The user's theme choices; `None` follows the system.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ThemePrefs {
    pub appearance: Option<Appearance>,
    pub accent: Option<Rgba>,
    /// Mica-like translucency: the base layer lets the (blurred) desktop show through. Only
    /// set this where the window really is blurred, or the desktop shows through unblurred.
    pub translucent: bool,
}

#[derive(Debug, Clone)]
pub struct Theme {
    /// The appearance in effect (system, unless the user or `ASTER_APPEARANCE` chose one).
    pub appearance: Appearance,
    /// The accent the colors were derived from (`None`: Aster's default blue).
    pub accent: Option<Rgba>,
    pub colors: Colors,
    system_appearance: Appearance,
    system_accent: Option<Rgba>,
    prefs: ThemePrefs,
}

impl Global for Theme {}

impl Theme {
    pub fn new(
        system_appearance: Appearance,
        system_accent: Option<Rgba>,
        prefs: ThemePrefs,
    ) -> Self {
        let appearance = forced_appearance()
            .or(prefs.appearance)
            .unwrap_or(system_appearance);
        let accent = prefs.accent.or(system_accent);
        let dark = appearance == Appearance::Dark;
        let colors = if dark {
            Colors::dark()
        } else {
            Colors::light()
        };
        Self {
            appearance,
            accent,
            colors: {
                let mut colors = match accent {
                    Some(a) => colors.with_accent(a, dark),
                    None => colors,
                };
                if prefs.translucent {
                    // Enough tint to keep sidebar text readable over any wallpaper.
                    colors.base.a = if dark { 0.80 } else { 0.72 };
                }
                colors
            },
            system_appearance,
            system_accent,
            prefs,
        }
    }

    /// The system's accent color, if it has one (shown as the "System" swatch in settings).
    pub fn system_accent(&self) -> Option<Rgba> {
        self.system_accent
    }

    pub fn is_dark(&self) -> bool {
        self.appearance == Appearance::Dark
    }

    /// Drop shadow for an elevation level.
    pub fn shadow(&self, level: Elevation) -> Vec<BoxShadow> {
        let (y, blur, light, dark) = match level {
            Elevation::Card => (2., 4., 0.04, 0.26),
            Elevation::Tooltip => (4., 8., 0.14, 0.26),
            Elevation::Flyout => (8., 16., 0.14, 0.26),
            Elevation::Dialog => (16., 32., 0.18, 0.37),
        };
        vec![BoxShadow {
            color: c(0x000000, if self.is_dark() { dark } else { light }),
            offset: point(px(0.), px(y)),
            blur_radius: px(blur),
            spread_radius: px(0.),
        }]
    }
}

/// Access to the current theme from any context.
pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        self.global::<Theme>()
    }
}

/// An appearance forced by `ASTER_APPEARANCE=light|dark` (overrides the system setting).
pub fn forced_appearance() -> Option<Appearance> {
    match std::env::var("ASTER_APPEARANCE")
        .ok()?
        .to_ascii_lowercase()
        .as_str()
    {
        "light" => Some(Appearance::Light),
        "dark" => Some(Appearance::Dark),
        _ => None,
    }
}

/// Install the theme for the system appearance and accent, with the user's choices on top.
pub fn init(system_appearance: impl Into<Appearance>, prefs: ThemePrefs, cx: &mut App) {
    cx.set_global(Theme::new(
        system_appearance.into(),
        accent::system_accent(),
        prefs,
    ));
}

/// Rebuild the theme with changed inputs. Returns whether what's on screen changes.
fn rebuild(
    cx: &mut App,
    f: impl FnOnce(&mut Appearance, &mut Option<Rgba>, &mut ThemePrefs),
) -> bool {
    let Some(current) = cx.try_global::<Theme>().cloned() else {
        return false;
    };
    let (mut appearance, mut accent, mut prefs) = (
        current.system_appearance,
        current.system_accent,
        current.prefs,
    );
    f(&mut appearance, &mut accent, &mut prefs);
    let next = Theme::new(appearance, accent, prefs);
    let visible = next.appearance != current.appearance
        || next.accent != current.accent
        || next.prefs.translucent != current.prefs.translucent;
    cx.set_global(next);
    visible
}

/// Follow a system appearance change. Returns whether what's on screen changes.
pub fn set_appearance(system_appearance: impl Into<Appearance>, cx: &mut App) -> bool {
    let system_appearance = system_appearance.into();
    rebuild(cx, |a, _, _| *a = system_appearance)
}

/// Re-read the system accent (e.g. when the app regains focus). Returns whether what's on
/// screen changes.
pub fn refresh_accent(cx: &mut App) -> bool {
    let system_accent = accent::system_accent();
    rebuild(cx, |_, a, _| *a = system_accent)
}

/// Apply the user's theme choices. Returns whether what's on screen changes.
pub fn set_prefs(prefs: ThemePrefs, cx: &mut App) -> bool {
    rebuild(cx, |_, _, p| *p = prefs)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::accent::contrast;

    #[test]
    fn text_contrast_meets_wcag_aa() {
        for colors in [Colors::light(), Colors::dark()] {
            for bg in [colors.base, colors.layer, colors.flyout] {
                assert!(contrast(colors.text_primary, bg) >= 4.5);
                assert!(contrast(colors.text_secondary, bg) >= 4.5);
                assert!(contrast(colors.accent, bg) >= 4.5, "accent on surface");
            }
            assert!(contrast(colors.critical, colors.critical_bg) >= 4.5);
            assert!(contrast(colors.success, colors.success_bg) >= 4.5);
            assert!(contrast(colors.caution, colors.caution_bg) >= 4.5);
        }
    }
}
