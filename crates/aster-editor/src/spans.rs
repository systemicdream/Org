//! Maps the Org syntax tree to styled byte ranges, then to GPUI text runs per line.

use std::ops::Range;

use aster_org::HeadingInfo;
use gpui::{
    FontStyle, FontWeight, Hsla, SharedString, StrikethroughStyle, TextRun, UnderlineStyle, px,
    rgb, rgba,
};

#[derive(Clone, Copy, Default)]
pub struct SpanStyle {
    pub color: Option<Hsla>,
    pub background: Option<Hsla>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub mono: bool,
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
    }
}

pub type Spans = Vec<(Range<usize>, SpanStyle)>;

fn color(hex: u32) -> Option<Hsla> {
    Some(rgb(hex).into())
}

const HEADING_COLORS: [u32; 4] = [0x1d4ed8, 0x7c3aed, 0x0f766e, 0xb45309];

/// Compute styled ranges for the whole document. Later spans override earlier ones.
pub fn style_spans(headings: &[HeadingInfo], text: &str) -> Spans {
    let mut spans = Spans::new();
    for h in headings {
        let line_end = text[h.range.start..]
            .find(['\r', '\n'])
            .map_or(text.len(), |i| h.range.start + i);
        let line = &text[h.range.start..line_end];
        let heading = SpanStyle {
            color: color(HEADING_COLORS[(h.level - 1) % HEADING_COLORS.len()]),
            bold: true,
            ..Default::default()
        };
        spans.push((h.range.start..line_end, heading));

        let muted = SpanStyle {
            color: color(0x9ca3af),
            ..heading
        };
        spans.push((h.range.start..h.range.start + h.level, muted));

        let after_stars = h.range.start + h.level;
        let rest = &text[after_stars..line_end];
        let keyword_start = after_stars + (rest.len() - rest.trim_start().len());
        if let Some(todo) = &h.todo
            && text[keyword_start..line_end].starts_with(todo.as_str())
        {
            // Pills hint that these are clickable.
            let (c, bg) = if h.done {
                (0x16a34a, 0xdcfce7ff)
            } else {
                (0xdc2626, 0xfee2e2ff)
            };
            spans.push((
                keyword_start..keyword_start + todo.len(),
                SpanStyle {
                    color: color(c),
                    background: Some(rgba(bg).into()),
                    bold: true,
                    ..Default::default()
                },
            ));
        }
        if let Some(p) = &h.priority
            && let Some(i) = line.find(&format!("[#{p}]"))
        {
            let start = h.range.start + i;
            spans.push((
                start..start + p.len() + 3,
                SpanStyle {
                    color: color(0xea580c),
                    background: Some(rgba(0xffedd5ff).into()),
                    bold: true,
                    ..Default::default()
                },
            ));
        }
        if !h.tags.is_empty() {
            let trimmed = line.trim_end();
            if trimmed.ends_with(':')
                && let Some(i) = trimmed.rfind(char::is_whitespace)
            {
                let start = h.range.start + i + 1;
                spans.push((
                    start..h.range.start + trimmed.len(),
                    SpanStyle {
                        color: color(0x6b7280),
                        background: Some(rgba(0xf3f4f6ff).into()),
                        ..Default::default()
                    },
                ));
            }
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
