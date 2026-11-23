//! Maps the Org syntax tree to styled byte ranges, then to GPUI text runs per line.

use std::ops::Range;

use gpui::{
    FontStyle, FontWeight, Hsla, SharedString, StrikethroughStyle, TextRun, UnderlineStyle, px,
};
use org_syntax::HeadingInfo;
use org_ui::Colors;

#[derive(Clone, Copy, Default)]
pub struct SpanStyle {
    pub color: Option<Hsla>,
    pub background: Option<Hsla>,
    pub bold: bool,
    /// Semibold (headings); `bold` wins when both apply.
    pub semibold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub mono: bool,
    /// Laid out but not drawn: something else is painted in its place (table rules,
    /// checkboxes).
    pub invisible: bool,
    /// Painted by the editor element rather than as text style (see [`Decor`]).
    pub decor: Option<Decor>,
}

/// Shapes the editor element paints around or in place of a span's text.
#[derive(Clone, Copy, PartialEq)]
pub enum Decor {
    /// A rounded label behind the text, in this color (TODO keywords, priorities, dates).
    Pill(Hsla),
    /// A checkbox drawn over the (invisible) checkbox glyph.
    Checkbox { checked: bool, partial: bool },
    /// An icon drawn in the space at the start of the span (a pill's leading em space).
    Icon(org_ui::icons::Icon),
}

impl SpanStyle {
    /// Apply this style on top of `run`.
    pub fn apply(&self, run: &mut TextRun, mono_family: &SharedString) {
        if let Some(c) = self.color {
            run.color = c;
        }
        if let Some(bg) = self.background {
            run.background_color = Some(bg);
        }
        if self.semibold && run.font.weight < FontWeight::SEMIBOLD {
            run.font.weight = FontWeight::SEMIBOLD;
        }
        if self.bold {
            run.font.weight = FontWeight::BOLD;
        }
        if self.italic {
            run.font.style = FontStyle::Italic;
        }
        if self.mono {
            run.font.family = mono_family.clone();
        }
        if self.underline {
            run.underline = Some(UnderlineStyle {
                color: Some(run.color),
                thickness: px(1.),
                wavy: false,
            });
        }
        if self.strike {
            run.strikethrough = Some(StrikethroughStyle {
                color: Some(run.color),
                thickness: px(1.),
            });
        }
        if self.invisible {
            run.color = gpui::transparent_black();
            run.background_color = None;
            run.underline = None;
            run.strikethrough = None;
        }
    }
}

pub type Spans = Vec<(Range<usize>, SpanStyle)>;

/// Compute styled ranges for the whole document. Later spans override earlier ones.
/// Colors follow docs/DESIGN.md §3 ("Org-specific colors").
pub fn style_spans(headings: &[HeadingInfo], text: &str, colors: &Colors) -> Spans {
    let mut spans = Spans::new();
    for h in headings {
        let line_end = text[h.range.start..]
            .find(['\r', '\n'])
            .map_or(text.len(), |i| h.range.start + i);
        let line = &text[h.range.start..line_end];
        // Headings are neutral; their size and weight carry the hierarchy. Done items fade.
        let heading = SpanStyle {
            color: Some(if h.done {
                colors.text_tertiary
            } else {
                colors.text_primary
            }),
            semibold: true,
            ..Default::default()
        };
        spans.push((h.range.start..line_end, heading));

        let muted = SpanStyle {
            color: Some(if h.level == 1 {
                colors.accent
            } else {
                colors.text_tertiary
            }),
            ..heading
        };
        spans.push((h.range.start..h.range.start + h.level, muted));

        let after_stars = h.range.start + h.level;
        let rest = &text[after_stars..line_end];
        let keyword_start = after_stars + (rest.len() - rest.trim_start().len());
        if let Some(todo) = &h.todo
            && text[keyword_start..line_end].starts_with(todo.as_str())
        {
            // Labels hint that these are clickable.
            let (c, bg) = if h.done {
                (colors.success, colors.success_bg)
            } else {
                (colors.critical, colors.critical_bg)
            };
            spans.push((
                keyword_start..keyword_start + todo.len(),
                SpanStyle {
                    color: Some(c),
                    decor: Some(Decor::Pill(bg)),
                    ..heading
                },
            ));
        }
        let mut title_start = h.title_start.min(line_end);
        if let Some(p) = &h.priority
            && let Some(i) = line.find(&format!("[#{p}]"))
        {
            let start = h.range.start + i;
            spans.push((
                start..start + p.len() + 3,
                SpanStyle {
                    color: Some(colors.caution),
                    decor: Some(Decor::Pill(colors.caution_bg)),
                    ..heading
                },
            ));
            title_start = title_start.max(start + p.len() + 3);
        }
        let mut title_end = h.range.start + line.trim_end().len();
        if !h.tags.is_empty() {
            let trimmed = line.trim_end();
            if trimmed.ends_with(':')
                && let Some(i) = trimmed.rfind(char::is_whitespace)
            {
                let start = h.range.start + i + 1;
                // Shown only while editing the heading line; otherwise drawn as labels.
                spans.push((
                    start..h.range.start + trimmed.len(),
                    SpanStyle {
                        color: Some(colors.text_secondary),
                        background: Some(colors.neutral_bg),
                        ..Default::default()
                    },
                ));
                title_end = h.range.start + line[..i].trim_end().len();
            }
        }
        // Finished headings are struck through (the title, not the keyword or tags).
        let title_start = title_start + text[title_start..line_end].len()
            - text[title_start..line_end].trim_start().len();
        if h.done && title_start < title_end {
            spans.push((
                title_start..title_end,
                SpanStyle {
                    strike: true,
                    ..heading
                },
            ));
        }
    }
    spans
}

/// Split `range` into text runs (lengths relative to `range.start`), applying overlapping spans
/// in order.
pub fn runs_for_range(
    base: &TextRun,
    spans: &Spans,
    range: Range<usize>,
    mono_family: &SharedString,
) -> Vec<TextRun> {
    let mut cuts = vec![range.start, range.end];
    for (r, _) in spans {
        for p in [r.start, r.end] {
            if p > range.start && p < range.end {
                cuts.push(p);
            }
        }
    }
    cuts.sort_unstable();
    cuts.dedup();

    let mut runs = Vec::new();
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let mut run = TextRun {
            len: b - a,
            ..base.clone()
        };
        for (r, style) in spans {
            if r.start <= a && b <= r.end {
                style.apply(&mut run, mono_family);
            }
        }
        runs.push(run);
    }
    runs
}
