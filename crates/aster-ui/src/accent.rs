//! The system accent color, and Fluent-style accent shades derived from it.
//!
//! Windows derives lighter/darker accent shades from the user's accent; we do the same: start
//! from the system color, then adjust its lightness per theme until it meets WCAG contrast on
//! the theme's surfaces (darker on light themes, lighter on dark themes).

use gpui::{Hsla, Rgba};

/// Read the user's accent color from the OS. `None` means "no preference" (e.g. macOS
/// "Multicolor"), in which case Aster uses its default Fluent blue.
pub fn system_accent() -> Option<Rgba> {
    // `ASTER_ACCENT=#rrggbb` overrides the system (handy for testing and personal taste).
    if let Some(c) = std::env::var("ASTER_ACCENT")
        .ok()
        .and_then(|v| parse_hex(&v))
    {
        return Some(c);
    }
    #[cfg(target_os = "macos")]
    {
        macos()
    }
    #[cfg(target_os = "linux")]
    {
        linux()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Accent colors offered in settings (alongside "System"), after the Windows 11 palette.
pub const SWATCHES: [(&str, u32); 10] = [
    ("Blue", 0x0078D4),
    ("Navy", 0x0063B1),
    ("Purple", 0x8764B8),
    ("Pink", 0xC239B3),
    ("Red", 0xE81123),
    ("Orange", 0xCA5010),
    ("Gold", 0xFFB900),
    ("Green", 0x107C10),
    ("Teal", 0x038387),
    ("Graphite", 0x69797E),
];

/// Parse `#rrggbb` (or `rrggbb`).
pub fn parse_hex(s: &str) -> Option<Rgba> {
    let s = s.trim().trim_start_matches('#');
    (s.len() == 6)
        .then(|| u32::from_str_radix(s, 16).ok())
        .flatten()
        .map(hex)
}

/// Format as `#rrggbb`.
pub fn to_hex(c: Rgba) -> String {
    let ch = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}", ch(c.r), ch(c.g), ch(c.b))
}

fn hex(v: u32) -> Rgba {
    gpui::rgb(v)
}

#[cfg(target_os = "macos")]
fn macos() -> Option<Rgba> {
    // `AppleAccentColor` is absent for "Multicolor" (apps use their own accent).
    let out = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleAccentColor"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let index: i32 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    // macOS system accent colors (controlAccentColor, light appearance).
    Some(hex(match index {
        -1 => 0x8C8C8C, // Graphite
        0 => 0xE0383E,  // Red
        1 => 0xF7821B,  // Orange
        2 => 0xFFC600,  // Yellow
        3 => 0x62BA46,  // Green
        4 => 0x007AFF,  // Blue
        5 => 0x953D96,  // Purple
        6 => 0xF74F9E,  // Pink
        _ => return None,
    }))
}

#[cfg(target_os = "linux")]
fn linux() -> Option<Rgba> {
    portal().or_else(kde).or_else(gnome)
}

/// XDG desktop portal (GNOME 47+, KDE Plasma 6, others): `org.freedesktop.appearance accent-color`.
#[cfg(target_os = "linux")]
fn portal() -> Option<Rgba> {
    let out = std::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--timeout",
            "2",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.ReadOne",
            "org.freedesktop.appearance",
            "accent-color",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_portal(&String::from_utf8_lossy(&out.stdout))
}

/// Parse gdbus output like `(<(0.21, 0.52, 0.89)>,)`. Out-of-range values mean "unset".
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // parsers are tested everywhere
fn parse_portal(s: &str) -> Option<Rgba> {
    let inner = s.split('(').next_back()?.split(')').next()?;
    let v: Vec<f32> = inner
        .split(',')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    if v.len() != 3 || v.iter().any(|c| !(0.0..=1.0).contains(c)) {
        return None;
    }
    Some(Rgba {
        r: v[0],
        g: v[1],
        b: v[2],
        a: 1.,
    })
}

/// KDE: `AccentColor=r,g,b` in `~/.config/kdeglobals`.
#[cfg(target_os = "linux")]
fn kde() -> Option<Rgba> {
    let path = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".config")))?
        .join("kdeglobals");
    parse_kde(&std::fs::read_to_string(path).ok()?)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // parsers are tested everywhere
fn parse_kde(text: &str) -> Option<Rgba> {
    let line = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("AccentColor="))?;
    let v: Vec<f32> = line
        .split(',')
        .filter_map(|p| p.trim().parse::<f32>().ok())
        .collect();
    (v.len() >= 3).then(|| Rgba {
        r: v[0] / 255.,
        g: v[1] / 255.,
        b: v[2] / 255.,
        a: 1.,
    })
}

/// GNOME 47+: a named accent (`gsettings get org.gnome.desktop.interface accent-color`).
#[cfg(target_os = "linux")]
fn gnome() -> Option<Rgba> {
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "accent-color"])
        .output()
        .ok()?;
    gnome_named(
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .trim_matches('\''),
    )
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // parsers are tested everywhere
fn gnome_named(name: &str) -> Option<Rgba> {
    Some(hex(match name {
        "blue" => 0x3584E4,
        "teal" => 0x2190A4,
        "green" => 0x3A944A,
        "yellow" => 0xC88800,
        "orange" => 0xED5B00,
        "red" => 0xE62D42,
        "pink" => 0xD56199,
        "purple" => 0x9141AC,
        "slate" => 0x6F8396,
        _ => return None,
    }))
}

/// Relative luminance per WCAG.
pub(crate) fn luminance(c: Hsla) -> f32 {
    let rgba: Rgba = c.into();
    let ch = |v: f32| {
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch(rgba.r) + 0.7152 * ch(rgba.g) + 0.0722 * ch(rgba.b)
}

pub(crate) fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The accent shade for a theme: `base` adjusted so it reads at ≥ 4.5:1 against every surface.
/// On dark themes it starts from a light tint (as Fluent's "Accent Light 2"), on light themes
/// from a darker shade ("Accent Dark 1").
pub fn derive(base: Rgba, dark: bool, surfaces: &[Hsla]) -> Hsla {
    let mut c: Hsla = base.into();
    c.a = 1.;
    c.l = if dark { c.l.max(0.62) } else { c.l.min(0.45) };
    for _ in 0..100 {
        if surfaces.iter().all(|s| contrast(c, *s) >= 4.5) {
            break;
        }
        c.l = (c.l + if dark { 0.01 } else { -0.01 }).clamp(0., 1.);
    }
    c
}

/// Black or white, whichever reads better on `accent`.
pub fn on_accent(accent: Hsla) -> Hsla {
    let white: Hsla = hex(0xFFFFFF).into();
    let black: Hsla = hex(0x000000).into();
    if contrast(white, accent) >= contrast(black, accent) {
        white
    } else {
        black
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Colors;

    #[test]
    fn parses_platform_formats() {
        let c = parse_portal("(<(0.2, 0.5, 1.0)>,)\n").unwrap();
        assert!((c.g - 0.5).abs() < 1e-6);
        assert!(parse_portal("(<(-1.0, -1.0, -1.0)>,)").is_none());
        let k = parse_kde("[General]\nAccentColor=61,174,233\nfoo=bar\n").unwrap();
        assert!((k.r - 61. / 255.).abs() < 1e-6);
        assert!(gnome_named("teal").is_some());
        assert!(gnome_named("chartreuse").is_none());
        let c = parse_hex("#038387").unwrap();
        assert_eq!(to_hex(c), "#038387");
        assert!(parse_hex("#12345").is_none());
    }

    #[test]
    fn derived_accents_meet_contrast() {
        // Including awkward accents: yellow, graphite, pale pink.
        let swatches = SWATCHES.map(|(_, c)| c);
        let system = [0xFFC600, 0x8C8C8C, 0xF74F9E, 0x007AFF, 0x62BA46, 0x2190A4];
        for base in system.into_iter().chain(swatches) {
            for (dark, colors) in [(false, Colors::light()), (true, Colors::dark())] {
                let surfaces = [colors.base, colors.layer, colors.flyout];
                let a = derive(hex(base), dark, &surfaces);
                for s in surfaces {
                    assert!(contrast(a, s) >= 4.5, "{base:06x} dark={dark}");
                }
                assert!(contrast(on_accent(a), a) >= 4.5, "{base:06x} on-accent");
            }
        }
    }
}
