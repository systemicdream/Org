//! Live-preview presentation: what kind of line each row is, how far it is indented, and
//! how inline markup is styled.

use std::ops::Range;

use aster_org::{BlockKind, DecoKind, Decoration, HeadingInfo};
use gpui::{Hsla, rgb, rgba};

use crate::spans::SpanStyle;

/// What a whole line is, for line-level styling.
#[derive(Debug, Clone, PartialEq)]
pub enum RowKind {
    Body,
    Heading(usize),
    Title,
    Keyword,
    /// `#+begin`/`#+end` line; true for code (source/example) blocks.
    BlockDelimiter(bool),
    /// Inside a source/example block; `lang` for syntax highlighting.
    Code(Option<String>),
    Quote,
    Drawer,
    Planning,
    Table,
    FixedWidth,
    Comment,
}

impl RowKind {
    /// Text size multiplier.
    pub fn scale(&self) -> f32 {
        match self {
            RowKind::Title => 1.8,
            RowKind::Heading(1) => 1.5,
            RowKind::Heading(2) => 1.3,
            RowKind::Heading(3) => 1.15,
            RowKind::Drawer | RowKind::Planning | RowKind::Keyword => 0.85,
            RowKind::Code(_)
            | RowKind::BlockDelimiter(_)
            | RowKind::Table
            | RowKind::FixedWidth => 0.9,
            _ => 1.0,
        }
    }

    /// Lines set in a monospace font.
    pub fn monospace(&self) -> bool {
        matches!(
            self,
            RowKind::Code(_) | RowKind::BlockDelimiter(_) | RowKind::Table | RowKind::FixedWidth
        )
    }

    /// Lines drawn with a tinted background (blocks).
    pub fn block_background(&self) -> bool {
        matches!(self, RowKind::Code(_) | RowKind::BlockDelimiter(true))
    }

    /// Lines drawn with a bar on the left (quotes).
    pub fn left_bar(&self) -> bool {
        *self == RowKind::Quote
    }

    /// Base style for the whole line.
    pub fn base_style(&self) -> SpanStyle {
        match self {
            RowKind::Title => SpanStyle {
                bold: true,
                color: Some(rgb(0x111827).into()),
                ..Default::default()
            },
            RowKind::Keyword
            | RowKind::Drawer
            | RowKind::Planning
            | RowKind::BlockDelimiter(_)
            | RowKind::Comment => SpanStyle {
                color: Some(muted()),
                italic: *self == RowKind::Comment,
                ..Default::default()
            },
            RowKind::Quote => SpanStyle {
                italic: true,
                color: Some(rgb(0x4b5563).into()),
                ..Default::default()
            },
            _ => SpanStyle::default(),
        }
    }
}

pub fn muted() -> Hsla {
    rgb(0x9ca3af).into()
}

/// Kind and outline depth (for indentation) of every line.
pub fn row_kinds(
    line_starts: &[usize],
    text_len: usize,
    headings: &[HeadingInfo],
    decorations: &[Decoration],
) -> Vec<(RowKind, usize)> {
    let mut rows: Vec<(RowKind, usize)> = vec![(RowKind::Body, 0); line_starts.len()];

    // Outline depth: body text sits one level deeper than its heading's text.
    let mut hi = 0;
    let mut depth = 0;
    for (row, &start) in line_starts.iter().enumerate() {
        while hi < headings.len() && headings[hi].range.start <= start {
            if headings[hi].range.start == start {
                rows[row].0 = RowKind::Heading(headings[hi].level);
            }
            depth = headings[hi].level;
            hi += 1;
        }
        rows[row].1 = match rows[row].0 {
            RowKind::Heading(level) => level - 1,
            _ => depth,
        };
    }

    for d in decorations.iter().filter(|d| d.is_line_level()) {
        let kind = match &d.kind {
            DecoKind::Title => RowKind::Title,
            DecoKind::Keyword => RowKind::Keyword,
            DecoKind::BlockDelimiter(block) => {
                RowKind::BlockDelimiter(matches!(block, BlockKind::Source | BlockKind::Example))
            }
            DecoKind::BlockContent { block, lang } => match block {
                BlockKind::Source | BlockKind::Example => RowKind::Code(lang.clone()),
                BlockKind::Quote | BlockKind::Verse => RowKind::Quote,
                BlockKind::Other => RowKind::Body,
            },
            DecoKind::Drawer => RowKind::Drawer,
            DecoKind::Planning => RowKind::Planning,
            DecoKind::Table => RowKind::Table,
            DecoKind::FixedWidth => RowKind::FixedWidth,
            DecoKind::Comment => RowKind::Comment,
            _ => continue,
        };
        for row in rows_in(line_starts, text_len, &d.range) {
            // Headings keep their kind; inner decorations (later, deeper) win otherwise.
            if !matches!(rows[row].0, RowKind::Heading(_)) {
                rows[row].0 = kind.clone();
            }
        }
    }
    rows
}

/// Rows whose line starts inside `range`.
fn rows_in(line_starts: &[usize], text_len: usize, range: &Range<usize>) -> Range<usize> {
    let first = line_starts.partition_point(|&s| s < range.start);
    let end = if range.end >= text_len {
        line_starts.len()
    } else {
        line_starts.partition_point(|&s| s < range.end)
    };
    first..end.max(first)
}

/// Inline style for a decoration in its decorated (not revealed) form.
pub fn inline_style(kind: &DecoKind) -> Option<SpanStyle> {
    let code_bg: Hsla = rgba(0xe5e7eb99).into();
    Some(match kind {
        DecoKind::Bold => SpanStyle {
            bold: true,
            ..Default::default()
        },
        DecoKind::Italic => SpanStyle {
            italic: true,
            ..Default::default()
        },
        DecoKind::Underline => SpanStyle {
            underline: true,
            ..Default::default()
        },
        DecoKind::Strike => SpanStyle {
            strike: true,
            color: Some(muted()),
            ..Default::default()
        },
        DecoKind::Verbatim | DecoKind::Code => SpanStyle {
            mono: true,
            background: Some(code_bg),
            color: Some(rgb(0xbe185d).into()),
            ..Default::default()
        },
        DecoKind::Link { .. } => SpanStyle {
            color: Some(rgb(0x2563eb).into()),
            underline: true,
            ..Default::default()
        },
        DecoKind::Timestamp => SpanStyle {
            color: Some(rgb(0x0f766e).into()),
            background: Some(rgba(0xccfbf199).into()),
            ..Default::default()
        },
        DecoKind::Checkbox { checked, .. } => SpanStyle {
            color: Some(
                if *checked {
                    rgb(0x16a34a)
                } else {
                    rgb(0x6b7280)
                }
                .into(),
            ),
            ..Default::default()
        },
        DecoKind::Bullet => SpanStyle {
            color: Some(rgb(0x6b7280).into()),
            ..Default::default()
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use aster_doc::Document;

    use super::*;

    #[test]
    fn kinds_and_depth() {
        let t = "#+TITLE: T\nintro\n* A\nbody\n** B\n#+begin_src rust\nx\n#+end_src\n| t |\n* C\n";
        let doc = Document::new(t);
        let mut starts = vec![0];
        starts.extend(t.match_indices('\n').map(|(i, _)| i + 1));
        let rows = row_kinds(&starts, t.len(), doc.headings(), doc.decorations());
        let kinds: Vec<_> = rows.iter().map(|(k, d)| (k.clone(), *d)).collect();
        assert_eq!(
            kinds,
            [
                (RowKind::Title, 0),
                (RowKind::Body, 0),
                (RowKind::Heading(1), 0),
                (RowKind::Body, 1),
                (RowKind::Heading(2), 1),
                (RowKind::BlockDelimiter(true), 2),
                (RowKind::Code(Some("rust".into())), 2),
                (RowKind::BlockDelimiter(true), 2),
                (RowKind::Table, 2),
                (RowKind::Heading(1), 0),
                (RowKind::Body, 1),
            ]
        );
    }
}
