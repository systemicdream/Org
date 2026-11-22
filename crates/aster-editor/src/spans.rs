//! Maps the Org syntax tree to styled byte ranges, then to GPUI text runs per line.

use std::ops::Range;

use aster_org::HeadingInfo;
use gpui::{FontWeight, Hsla, TextRun, UnderlineStyle, px, rgb};

#[derive(Clone, Copy, Default)]
pub struct SpanStyle {
    pub color: Option<Hsla>,
    pub background: Option<Hsla>,
    pub bold: bool,
    pub underline: bool,
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
            let c = if h.done { 0x16a34a } else { 0xdc2626 };
            spans.push((
                keyword_start..keyword_start + todo.len(),
                SpanStyle {
                    color: color(c),
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
                        ..Default::default()
                    },
                ));
            }
        }
    }
    spans
}

/// Split `range` into text runs (lengths relative to `range.start`), applying overlapping spans.
pub fn runs_for_range(base: &TextRun, spans: &Spans, range: Range<usize>) -> Vec<TextRun> {
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
                if let Some(c) = style.color {
                    run.color = c;
                }
                if let Some(bg) = style.background {
                    run.background_color = Some(bg);
                }
                if style.bold {
                    run.font.weight = FontWeight::BOLD;
                }
                if style.underline {
                    run.underline = Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.),
                        wavy: false,
                    });
                }
            }
        }
        runs.push(run);
    }
    runs
}
