//! Live-preview presentation: what kind of line each row is, how far it is indented, and
//! how inline markup is styled.

use std::ops::Range;

use aster_doc::dates::Timestamp;
use aster_org::{BlockKind, DecoKind, Decoration, HeadingInfo};
use aster_ui::Colors;
use chrono::{Datelike, NaiveDate};

use crate::spans::{Decor, SpanStyle};

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
    /// A table row; `header` for rows above the table's first rule.
    Table {
        header: bool,
    },
    /// A table rule (`|---+---|`).
    TableRule,
    FixedWidth,
    Comment,
}

impl RowKind {
    /// Text size multiplier.
    pub fn scale(&self) -> f32 {
        match self {
            RowKind::Title => 1.75,
            RowKind::Heading(1) => 1.35,
            RowKind::Heading(2) => 1.18,
            RowKind::Heading(3) => 1.08,
            RowKind::Drawer | RowKind::Planning | RowKind::Keyword => 0.85,
            RowKind::Code(_)
            | RowKind::BlockDelimiter(_)
            | RowKind::Table { .. }
            | RowKind::TableRule
            | RowKind::FixedWidth => 0.9,
            _ => 1.0,
        }
    }

    /// Lines set in a monospace font.
    pub fn monospace(&self) -> bool {
        matches!(
            self,
            RowKind::Code(_)
                | RowKind::BlockDelimiter(_)
                | RowKind::Table { .. }
                | RowKind::TableRule
                | RowKind::FixedWidth
        )
    }

    pub fn is_table(&self) -> bool {
        matches!(self, RowKind::Table { .. } | RowKind::TableRule)
    }

    /// Space above the row, in line heights: headings get room to breathe, more for higher
    /// levels, and the first row after the title is set apart from it.
    pub fn space_above(&self, previous: &RowKind, previous_blank: bool) -> f32 {
        let space = match self {
            RowKind::Heading(1) => 0.9,
            RowKind::Heading(2) => 0.45,
            RowKind::Heading(_) => 0.3,
            _ if *previous == RowKind::Title => 0.5,
            _ => 0.,
        };
        // A blank line already separates them.
        if previous_blank { space * 0.4 } else { space }
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
    pub fn base_style(&self, colors: &Colors) -> SpanStyle {
        match self {
            RowKind::Title => SpanStyle {
                semibold: true,
                color: Some(colors.text_primary),
                ..Default::default()
            },
            RowKind::Keyword
            | RowKind::Drawer
            | RowKind::Planning
            | RowKind::BlockDelimiter(_)
            | RowKind::Comment => SpanStyle {
                color: Some(colors.text_tertiary),
                italic: *self == RowKind::Comment,
                ..Default::default()
            },
            RowKind::Quote => SpanStyle {
                italic: true,
                color: Some(colors.text_secondary),
                ..Default::default()
            },
            _ => SpanStyle::default(),
        }
    }
}

/// Kind and outline depth (for indentation) of every line.
pub fn row_kinds(
    text: &str,
    line_starts: &[usize],
    headings: &[HeadingInfo],
    decorations: &[Decoration],
) -> Vec<(RowKind, usize)> {
    let text_len = text.len();
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
            DecoKind::Table => {
                // Rows above the first rule are the header (when there is a rule below them).
                let rows_range = rows_in(line_starts, text_len, &d.range);
                let is_rule = |row: usize| text[line_starts[row]..].trim_start().starts_with("|-");
                let first_rule = rows_range.clone().find(|&r| is_rule(r));
                // Only lines starting with `|` (the table's range can include a blank line after).
                let is_row = |row: usize| text[line_starts[row]..].trim_start().starts_with('|');
                for row in rows_range.filter(|&r| is_row(r)) {
                    if !matches!(rows[row].0, RowKind::Heading(_)) {
                        rows[row].0 = if is_rule(row) {
                            RowKind::TableRule
                        } else {
                            RowKind::Table {
                                header: first_rule.is_some_and(|f| row < f),
                            }
                        };
                    }
                }
                continue;
            }
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
    // A line starting exactly at the range end (e.g. the empty line after a final newline)
    // is outside it.
    let end = line_starts.partition_point(|&s| s < range.end.min(text_len));
    first..end.max(first)
}

/// A date as people say it: "Fri, Nov 27", "today", with the year only when it isn't this
/// year's, the time if there is one, and ↻ for a repeating date. `label` goes first ("Due
/// tomorrow").
pub fn friendly_date(ts: &Timestamp, label: Option<&str>, today: NaiveDate) -> String {
    let day = match (ts.date - today).num_days() {
        0 => "today".to_string(),
        1 => "tomorrow".to_string(),
        -1 => "yesterday".to_string(),
        _ if ts.date.year() == today.year() => ts.date.format("%a, %b %-d").to_string(),
        _ => ts.date.format("%a, %b %-d, %Y").to_string(),
    };
    let mut s = match label {
        Some(label) => format!("{label} {day}"),
        None => {
            let mut c = day.chars();
            c.next()
                .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
        }
    };
    if let Some(time) = &ts.time {
        s.push(' ');
        s.push_str(time);
    }
    if ts.repeater.is_some() {
        s.push_str(" ↻");
    }
    s
}

/// Inline style for a decoration in its decorated (not revealed) form.
pub fn inline_style(kind: &DecoKind, colors: &Colors) -> Option<SpanStyle> {
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
            color: Some(colors.text_tertiary),
            ..Default::default()
        },
        DecoKind::Verbatim | DecoKind::Code => SpanStyle {
            mono: true,
            background: Some(colors.subtle_secondary),
            color: Some(colors.text_primary),
            ..Default::default()
        },
        DecoKind::Link { .. } => SpanStyle {
            color: Some(colors.accent),
            underline: true,
            ..Default::default()
        },
        // Dates are styled by the editor (it knows today's date and the heading's state).
        DecoKind::Checkbox { checked, partial } => SpanStyle {
            invisible: true,
            decor: Some(Decor::Checkbox {
                checked: *checked,
                partial: *partial,
            }),
            ..Default::default()
        },
        DecoKind::Bullet => SpanStyle {
            color: Some(colors.text_tertiary),
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
    fn friendly_dates() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        let f = |s: &str, label| friendly_date(&Timestamp::parse(s).unwrap(), label, today);
        assert_eq!(f("<2026-11-27 Fri>", Some("Due")), "Due Fri, Nov 27");
        assert_eq!(f("<2026-11-23 Mon>", Some("Due")), "Due today");
        assert_eq!(f("<2026-11-24 Tue 10:00>", None), "Tomorrow 10:00");
        assert_eq!(f("[2026-11-22 Sun]", Some("Done")), "Done yesterday");
        assert_eq!(f("<2027-01-04 Mon +1w>", None), "Mon, Jan 4, 2027 ↻");
    }

    #[test]
    fn kinds_and_depth() {
        let t = "#+TITLE: T\nintro\n* A\nbody\n** B\n#+begin_src rust\nx\n#+end_src\n| h |\n|---|\n| t |\n* C\n";
        let doc = Document::new(t);
        let mut starts = vec![0];
        starts.extend(t.match_indices('\n').map(|(i, _)| i + 1));
        let rows = row_kinds(t, &starts, doc.headings(), doc.decorations());
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
                (RowKind::Table { header: true }, 2),
                (RowKind::TableRule, 2),
                (RowKind::Table { header: false }, 2),
                (RowKind::Heading(1), 0),
                (RowKind::Body, 1),
            ]
        );
    }
}
