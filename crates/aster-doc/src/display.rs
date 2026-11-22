//! Mapping between a line's buffer text and what live preview displays.
//!
//! Hidden markup disappears from the display text and replaced markup (e.g. `[ ]`) shows as a
//! glyph (`☐`). Offsets convert both ways so cursor movement, selection and hit-testing work
//! on the display while edits still apply to the buffer.

use std::ops::Range;

use aster_org::Decoration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// Absolute buffer range.
    pub buf: Range<usize>,
    /// Range in the display text.
    pub disp: Range<usize>,
    /// Shown as a replacement glyph rather than the buffer text.
    pub replaced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowDisplay {
    /// Buffer range of the line (without its terminator).
    pub line: Range<usize>,
    pub text: String,
    pub segments: Vec<Segment>,
}

impl RowDisplay {
    /// Build the display for `line`, hiding/replacing markup of decorations not revealed.
    pub fn build<'a>(
        doc: &str,
        line: Range<usize>,
        decorations: impl IntoIterator<Item = &'a Decoration>,
        revealed: impl Fn(&Decoration) -> bool,
    ) -> Self {
        // (range, replacement) edits within the line, sorted, non-overlapping.
        let mut edits: Vec<(Range<usize>, Option<&'static str>)> = Vec::new();
        for d in decorations {
            if revealed(d) {
                continue;
            }
            for h in &d.hide {
                edits.push((h.clone(), None));
            }
            if let Some((r, glyph)) = &d.replace {
                edits.push((r.clone(), Some(*glyph)));
            }
        }
        let mut edits: Vec<_> = edits
            .into_iter()
            .map(|(r, g)| (r.start.max(line.start)..r.end.min(line.end), g))
            .filter(|(r, _)| !r.is_empty())
            .collect();
        edits.sort_by_key(|(r, _)| r.start);

        let mut text = String::new();
        let mut segments = Vec::new();
        let mut pos = line.start;
        let push_text = |text: &mut String, segments: &mut Vec<Segment>, r: Range<usize>| {
            if r.is_empty() {
                return;
            }
            let d = text.len();
            text.push_str(&doc[r.clone()]);
            segments.push(Segment {
                disp: d..text.len(),
                buf: r,
                replaced: false,
            });
        };
        for (r, glyph) in edits {
            if r.start < pos {
                continue; // overlapping edit; the earlier one wins
            }
            push_text(&mut text, &mut segments, pos..r.start);
            if let Some(g) = glyph {
                let d = text.len();
                text.push_str(g);
                segments.push(Segment {
                    disp: d..text.len(),
                    buf: r.clone(),
                    replaced: true,
                });
            }
            pos = r.end;
        }
        push_text(&mut text, &mut segments, pos..line.end);
        Self {
            line,
            text,
            segments,
        }
    }

    /// The whole line shown as `label` (e.g. a collapsed drawer's chip).
    pub fn replaced_line(line: Range<usize>, label: &str) -> Self {
        Self {
            segments: vec![Segment {
                buf: line.clone(),
                disp: 0..label.len(),
                replaced: true,
            }],
            text: label.to_string(),
            line,
        }
    }

    /// Display identical to the buffer line.
    pub fn plain(doc: &str, line: Range<usize>) -> Self {
        Self::build(doc, line, std::iter::empty(), |_| true)
    }

    /// Display index for an absolute buffer offset on this line. Offsets inside hidden text map
    /// to where that text would have been.
    pub fn to_display(&self, buf: usize) -> usize {
        for s in &self.segments {
            if buf < s.buf.start {
                return s.disp.start;
            }
            if buf < s.buf.end {
                return if s.replaced {
                    s.disp.start
                } else {
                    s.disp.start + (buf - s.buf.start)
                };
            }
        }
        self.text.len()
    }

    /// Absolute buffer offset for a display index. At a boundary where markup is hidden, the
    /// offset after the hidden markup is preferred (so clicking after `bold` lands after `*`).
    pub fn to_buffer(&self, disp: usize) -> usize {
        let mut last_end = self.line.start;
        for s in &self.segments {
            if disp < s.disp.end || (disp == s.disp.start) {
                if disp <= s.disp.start {
                    return s.buf.start;
                }
                return if s.replaced {
                    s.buf.end
                } else {
                    s.buf.start + (disp - s.disp.start)
                };
            }
            last_end = s.buf.end;
        }
        if self.segments.last().is_some_and(|s| disp >= s.disp.end) {
            // After the last visible text: skip trailing hidden markup only if the line ends
            // with it, so the cursor sits at the very end of the line.
            return self.line.end.max(last_end);
        }
        self.line.end
    }

    /// Buffer range that a display range covers (for selections and IME bounds).
    pub fn display_range(&self, buf: Range<usize>) -> Range<usize> {
        self.to_display(buf.start)..self.to_display(buf.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    fn row(text: &str, reveal_all: bool) -> RowDisplay {
        let doc = Document::new(text);
        let decos = doc.decorations().to_vec();
        let end = text.find('\n').unwrap_or(text.len());
        RowDisplay::build(text, 0..end, &decos, |_| reveal_all)
    }

    #[test]
    fn hides_and_replaces() {
        assert_eq!(row("a *b* c", false).text, "a b c");
        assert_eq!(row("a *b* c", true).text, "a *b* c");
        assert_eq!(
            row("see [[https://x.org][site]] now", false).text,
            "see site now"
        );
        assert_eq!(row("- [ ] task", false).text, "• ☐ task");
        assert_eq!(row("* TODO Head", false).text, "TODO Head");
        assert_eq!(row("#+TITLE: Notes", false).text, "Notes");
    }

    #[test]
    fn offsets_round_trip() {
        let t = "a *bold* c";
        let r = row(t, false); // "a bold c"
        assert_eq!(r.to_display(0), 0);
        assert_eq!(r.to_display(2), 2); // the hidden '*' maps to where it was
        assert_eq!(r.to_display(3), 2); // 'b'
        assert_eq!(r.to_display(7), 6); // closing '*'
        assert_eq!(r.to_display(t.len()), r.text.len());
        assert_eq!(r.to_buffer(2), 3); // before 'b' -> after the opening '*'
        assert_eq!(r.to_buffer(6), 8); // after 'bold' -> after the closing '*'
        assert_eq!(r.to_buffer(r.text.len()), t.len());
        for d in 0..=r.text.len() {
            assert_eq!(r.to_display(r.to_buffer(d)), d, "display {d}");
        }
    }

    #[test]
    fn replaced_glyph_offsets() {
        let t = "- [ ] task";
        let r = row(t, false); // "• ☐ task"
        let glyph = "•".len();
        assert_eq!(r.to_buffer(0), 0);
        assert_eq!(r.to_buffer(glyph), 1);
        assert_eq!(r.to_display(6), r.text.find("task").unwrap());
        for d in [0, glyph, r.text.find("☐").unwrap(), r.text.len()] {
            assert_eq!(r.to_display(r.to_buffer(d)), d, "display {d}");
        }
    }

    #[test]
    fn plain_is_identity() {
        let r = RowDisplay::plain("hello", 0..5);
        assert_eq!(r.text, "hello");
        for i in 0..=5 {
            assert_eq!(r.to_display(i), i);
            assert_eq!(r.to_buffer(i), i);
        }
    }
}
