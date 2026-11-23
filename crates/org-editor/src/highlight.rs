//! Syntax highlighting for source blocks (syntect, Sublime grammars).

use std::ops::Range;
use std::sync::OnceLock;

use gpui::{Hsla, Rgba};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::spans::SpanStyle;

struct Assets {
    syntaxes: SyntaxSet,
    light: Theme,
    dark: Theme,
}

fn assets() -> &'static Assets {
    static ASSETS: OnceLock<Assets> = OnceLock::new();
    ASSETS.get_or_init(|| {
        let mut themes = ThemeSet::load_defaults();
        Assets {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            light: themes
                .themes
                .remove("InspiredGitHub")
                .expect("bundled theme"),
            dark: themes
                .themes
                .remove("base16-ocean.dark")
                .expect("bundled theme"),
        }
    })
}

/// Map Org block language names to syntect tokens.
fn syntax_for(lang: &str) -> Option<&'static SyntaxReference> {
    let ss = &assets().syntaxes;
    let token = match lang.to_ascii_lowercase().as_str() {
        "sh" | "shell" | "bash" | "zsh" => "bash".to_string(),
        "elisp" | "emacs-lisp" | "lisp" | "scheme" | "clojure" => "lisp".to_string(),
        "js" | "javascript" | "jsx" => "js".to_string(),
        "ts" | "typescript" => "js".to_string(),
        "py" | "python" => "py".to_string(),
        "c++" | "cpp" => "cpp".to_string(),
        "md" | "markdown" => "md".to_string(),
        other => other.to_string(),
    };
    ss.find_syntax_by_token(&token)
}

fn color(c: syntect::highlighting::Color) -> Hsla {
    Rgba {
        r: c.r as f32 / 255.,
        g: c.g as f32 / 255.,
        b: c.b as f32 / 255.,
        a: c.a as f32 / 255.,
    }
    .into()
}

/// Highlight `code` (a block's content) as `lang`. Ranges are relative to `code`.
pub fn highlight(lang: &str, code: &str, dark: bool) -> Vec<(Range<usize>, SpanStyle)> {
    let Some(syntax) = syntax_for(lang) else {
        return Vec::new();
    };
    let a = assets();
    let mut h = HighlightLines::new(syntax, if dark { &a.dark } else { &a.light });
    let mut out = Vec::new();
    let mut offset = 0;
    for line in code.split_inclusive('\n') {
        let Ok(regions) = h.highlight_line(line, &a.syntaxes) else {
            break;
        };
        let mut pos = offset;
        for (style, piece) in regions {
            let range = pos..pos + piece.len();
            pos = range.end;
            if piece.trim().is_empty() {
                continue;
            }
            out.push((
                range,
                SpanStyle {
                    color: Some(color(style.foreground)),
                    bold: style.font_style.contains(FontStyle::BOLD),
                    italic: style.font_style.contains(FontStyle::ITALIC),
                    ..Default::default()
                },
            ));
        }
        offset += line.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_known_languages() {
        let spans = highlight("rust", "fn main() {\n    let x = 1;\n}\n", false);
        assert!(!spans.is_empty());
        let colors: std::collections::HashSet<_> = spans
            .iter()
            .map(|(_, s)| format!("{:?}", s.color))
            .collect();
        assert!(colors.len() > 1, "expected several colors");
        assert!(!highlight("python", "def f():\n    return 1\n", true).is_empty());
        assert!(!highlight("sh", "echo hi\n", false).is_empty());
        assert!(highlight("not-a-language", "x", false).is_empty());
    }
}
