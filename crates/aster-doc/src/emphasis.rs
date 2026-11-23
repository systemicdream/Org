//! Inline formatting of a selection: bold, italic, underline, strikethrough, code and links.
//!
//! Org emphasis markers only count at word boundaries and can't hug whitespace, so a selection
//! is widened to whole words and trimmed before it's wrapped, and each line is wrapped on its
//! own. Heading stars, TODO keywords, priorities, tags, list bullets and checkboxes are never
//! wrapped.

use std::ops::Range;

use aster_org::{DecoKind, Decoration};

use crate::ops::OpResult;
use crate::{Document, Selection, Transaction};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Emphasis {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
}

impl Emphasis {
    pub const ALL: [Emphasis; 5] = [
        Emphasis::Bold,
        Emphasis::Italic,
        Emphasis::Underline,
        Emphasis::Strike,
        Emphasis::Code,
    ];

    pub fn marker(self) -> char {
        match self {
            Emphasis::Bold => '*',
            Emphasis::Italic => '/',
            Emphasis::Underline => '_',
            Emphasis::Strike => '+',
            Emphasis::Code => '~',
        }
    }

    fn matches(self, kind: &DecoKind) -> bool {
        matches!(
            (self, kind),
            (Emphasis::Bold, DecoKind::Bold)
                | (Emphasis::Italic, DecoKind::Italic)
                | (Emphasis::Underline, DecoKind::Underline)
                | (Emphasis::Strike, DecoKind::Strike)
                | (Emphasis::Code, DecoKind::Code | DecoKind::Verbatim)
        )
    }
}

/// The `e` element containing all of `range`, if any.
fn containing<'a>(doc: &'a Document, range: &Range<usize>, e: Emphasis) -> Option<&'a Decoration> {
    doc.decorations().iter().find(|d| {
        e.matches(&d.kind)
            && d.hide.len() == 2
            && d.range.start <= range.start
            && range.end <= d.range.end
    })
}

/// Formatting that applies to all of `range` (for showing which toolbar buttons are on).
pub fn active(doc: &Document, range: Range<usize>) -> Vec<Emphasis> {
    Emphasis::ALL
        .into_iter()
        .filter(|&e| containing(doc, &range, e).is_some())
        .collect()
}

/// Where a line's own text starts and ends: after heading stars, keyword and priority, list
/// bullets and checkboxes; before heading tags.
fn content_range(doc: &Document, line: Range<usize>) -> Range<usize> {
    let text = doc.text();
    let s = &text[line.clone()];
    if let Some(h) = doc.headings().iter().find(|h| h.range.start == line.start) {
        let mut start = h.title_start.clamp(line.start, line.end);
        let rest = &text[start..line.end];
        if rest.starts_with("[#") && rest.as_bytes().get(3) == Some(&b']') {
            start += 4;
        }
        let mut end = line.start + s.trim_end().len();
        let trimmed = s.trim_end();
        if !h.tags.is_empty()
            && trimmed.ends_with(':')
            && let Some(i) = trimmed.rfind(char::is_whitespace)
        {
            end = line.start + i;
        }
        return start.min(end)..end;
    }
    let indent = s.len() - s.trim_start().len();
    let rest = &s[indent..];
    let bullet = if rest.starts_with("- ") || rest.starts_with("+ ") {
        2
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && (rest[digits..].starts_with(". ") || rest[digits..].starts_with(") ")) {
            digits + 2
        } else {
            0
        }
    };
    let mut start = indent + bullet;
    if bullet > 0 {
        for b in ["[ ] ", "[X] ", "[x] ", "[-] "] {
            if s[start..].starts_with(b) {
                start += b.len();
            }
        }
    }
    line.start + start..line.end
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

/// `range` on one line, clamped to the line's content, widened to whole words and trimmed.
fn wrap_range(doc: &Document, line: Range<usize>, range: Range<usize>) -> Option<Range<usize>> {
    let text = doc.text();
    let content = content_range(doc, line);
    let mut start = range.start.clamp(content.start, content.end);
    let mut end = range.end.clamp(content.start, content.end);
    // A selection edge inside a word moves out to the word's edge; a bare cursor takes the
    // whole word it touches.
    let cursor = start == end;
    while let Some(c) = text[content.start..start].chars().next_back()
        && is_word(c)
        && (cursor || text[start..].chars().next().is_some_and(is_word))
    {
        start -= c.len_utf8();
    }
    while let Some(c) = text[end..content.end].chars().next()
        && is_word(c)
        && (cursor || text[..end].chars().next_back().is_some_and(is_word))
    {
        end += c.len_utf8();
    }
    let inner = &text[start..end];
    let start = start + (inner.len() - inner.trim_start().len());
    let end = start.max(start + inner.trim().len());
    (start < end).then_some(start..end)
}

/// Toggle `e` on the selection (or, with none, the word at the cursor): remove it if the
/// selection is already inside such an element, otherwise wrap each selected line's text in
/// its markers.
pub fn toggle(doc: &Document, sel: Selection, e: Emphasis) -> Option<OpResult> {
    let range = sel.range();
    if let Some(d) = containing(doc, &range, e) {
        let tx = Transaction::new()
            .with(d.hide[0].clone(), "")
            .with(d.hide[1].clone(), "");
        return Some(OpResult::mapped(tx, sel));
    }
    let text = doc.text();
    let mut segments = Vec::new();
    let mut line_start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
    while line_start <= range.end {
        let line_end = text[line_start..]
            .find('\n')
            .map_or(text.len(), |i| line_start + i);
        let line_end_trim = if text[..line_end].ends_with('\r') {
            line_end - 1
        } else {
            line_end
        };
        if let Some(r) = wrap_range(
            doc,
            line_start..line_end_trim,
            range.start.max(line_start)..range.end.min(line_end_trim),
        ) {
            segments.push(r);
        }
        if line_end >= text.len() {
            break;
        }
        line_start = line_end + 1;
    }
    let (first, last) = (segments.first()?.start, segments.last()?.end);
    let marker = e.marker().to_string();
    let mut tx = Transaction::new();
    // The same formatting inside the new element would end it early; drop its markers.
    let inner: Vec<_> = doc
        .decorations()
        .iter()
        .filter(|d| {
            e.matches(&d.kind)
                && d.hide.len() == 2
                && segments
                    .iter()
                    .any(|s| s.start <= d.range.start && d.range.end <= s.end)
        })
        .collect();
    for d in inner {
        tx = tx.with(d.hide[0].clone(), "").with(d.hide[1].clone(), "");
    }
    for s in &segments {
        tx = tx
            .with(s.start..s.start, marker.clone())
            .with(s.end..s.end, marker.clone());
    }
    let (a, h) = (tx.map(first, false), tx.map(last, true));
    // No selection (formatting the word at the cursor): the cursor stays where it was.
    let selection = if sel.is_empty() {
        Selection::cursor(tx.map(sel.head, false))
    } else if sel.head < sel.anchor {
        Selection { anchor: h, head: a }
    } else {
        Selection { anchor: a, head: h }
    };
    Some(OpResult { tx, selection })
}

/// Turn the selection into a link's description, `[[|][text]]`, with the cursor where the
/// address goes.
pub fn link(doc: &Document, sel: Selection) -> Option<OpResult> {
    let range = sel.range();
    let text = doc.text();
    if range.is_empty() || text[range.clone()].contains('\n') {
        return None;
    }
    // Whole words, like the other formatting.
    let line_start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[range.end..]
        .find(['\r', '\n'])
        .map_or(text.len(), |i| range.end + i);
    let range = wrap_range(doc, line_start..line_end, range)?;
    let tx = Transaction::new()
        .with(range.start..range.start, "[[][")
        .with(range.end..range.end, "]]");
    Some(OpResult {
        tx,
        selection: Selection::cursor(range.start + 2),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, sel: Range<usize>, e: Emphasis) -> (String, String) {
        let doc = Document::new(text);
        let r = toggle(
            &doc,
            Selection {
                anchor: sel.start,
                head: sel.end,
            },
            e,
        )
        .unwrap();
        let mut out = text.to_string();
        r.tx.apply(&mut out);
        let selected = out[r.selection.range()].to_string();
        (out, selected)
    }

    #[test]
    fn wraps_whole_words() {
        let t = "say hello world now\n";
        let (out, sel) = run(t, 5..13, Emphasis::Bold); // "ello wor"
        assert_eq!(out, "say *hello world* now\n");
        assert_eq!(sel, "hello world");
        // Trailing space trimmed.
        let (out, _) = run(t, 4..10, Emphasis::Italic);
        assert_eq!(out, "say /hello/ world now\n");
    }

    #[test]
    fn unwraps_when_inside() {
        let t = "a *bold* b\n";
        let (out, sel) = run(t, 3..7, Emphasis::Bold);
        assert_eq!(out, "a bold b\n");
        assert_eq!(sel, "bold");
        let doc = Document::new(t);
        assert_eq!(active(&doc, 4..6), [Emphasis::Bold]);
        assert!(active(&doc, 0..1).is_empty());
    }

    #[test]
    fn skips_heading_and_list_markup() {
        let t = "* TODO [#A] Buy milk :shop:\n- [ ] eggs and ham\n";
        let (out, _) = run(t, 0..t.len() - 1, Emphasis::Bold);
        assert_eq!(out, "* TODO [#A] *Buy milk* :shop:\n- [ ] *eggs and ham*\n");
    }

    #[test]
    fn merges_inner_same_formatting() {
        let t = "one *two* three\n";
        let (out, _) = run(t, 0..15, Emphasis::Bold);
        assert_eq!(out, "*one two three*\n");
    }

    #[test]
    fn word_at_cursor() {
        let t = "say hello now\n";
        let doc = Document::new(t);
        let r = toggle(&doc, Selection::cursor(6), Emphasis::Bold).unwrap();
        let mut out = t.to_string();
        r.tx.apply(&mut out);
        assert_eq!(out, "say *hello* now\n");
        assert_eq!(r.selection, Selection::cursor(7));
        // At either edge of a word, that word.
        let r = toggle(&doc, Selection::cursor(4), Emphasis::Italic).unwrap();
        let mut out = t.to_string();
        r.tx.apply(&mut out);
        assert_eq!(out, "say /hello/ now\n");
        // Between spaces: nothing to format.
        assert!(
            toggle(
                &Document::new("a  b\n"),
                Selection::cursor(2),
                Emphasis::Bold
            )
            .is_none()
        );
    }

    #[test]
    fn links_selection() {
        let doc = Document::new("see site\n");
        let r = link(&doc, Selection { anchor: 4, head: 8 }).unwrap();
        let mut out = "see site\n".to_string();
        r.tx.apply(&mut out);
        assert_eq!(out, "see [[][site]]\n");
        assert_eq!(r.selection, Selection::cursor(6));
    }
}
