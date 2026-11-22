//! Structural Org commands. Each takes the document and the current selection and returns the
//! transaction to apply plus the selection afterwards, or `None` when it doesn't apply.
//!
//! These are pure functions over text + tree, so they're tested without any UI.

use std::ops::Range;

use aster_org::HeadingInfo;

use crate::{Document, Selection, Transaction};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpResult {
    pub tx: Transaction,
    pub selection: Selection,
}

impl OpResult {
    /// Keep the selection on the same text, staying before text inserted at its position.
    fn mapped(tx: Transaction, sel: Selection) -> Self {
        let selection = Selection {
            anchor: tx.map(sel.anchor, true),
            head: tx.map(sel.head, true),
        };
        Self { tx, selection }
    }
}

/// Index of the innermost heading whose section contains `offset`.
fn heading_at(doc: &Document, headings: &[HeadingInfo], offset: usize) -> Option<usize> {
    let len = doc.text().len();
    headings.iter().rposition(|h| {
        h.range.start <= offset && (offset < h.range.end || (offset == len && h.range.end == len))
    })
}

fn line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Cycle the TODO state of the heading at the cursor:
/// none → first TODO keyword → … → last DONE keyword → none (reverse when `forward` is false).
pub fn cycle_todo(doc: &Document, sel: Selection, forward: bool) -> Option<OpResult> {
    let headings = doc.tree().headings();
    let h = &headings[heading_at(doc, &headings, sel.head)?];
    let (todo, done) = doc.tree().todo_keywords();
    let states: Vec<&str> = todo.iter().chain(done).map(String::as_str).collect();
    if states.is_empty() {
        return None;
    }
    let current = h
        .todo
        .as_deref()
        .and_then(|t| states.iter().position(|s| *s == t));
    let next = match (current, forward) {
        (None, true) => Some(0),
        (None, false) => Some(states.len() - 1),
        (Some(i), true) => (i + 1 < states.len()).then_some(i + 1),
        (Some(i), false) => i.checked_sub(1),
    }
    .map(|i| states[i]);

    let tx = match (&h.todo_range, next) {
        (Some(r), Some(k)) => Transaction::replace(r.clone(), k),
        (Some(r), None) => Transaction::replace(r.start..h.title_start.max(r.end), ""),
        (None, Some(k)) if h.title_start == h.line_end => {
            Transaction::replace(h.title_start..h.title_start, k)
        }
        (None, Some(k)) => Transaction::replace(h.title_start..h.title_start, format!("{k} ")),
        (None, None) => return None,
    };
    Some(OpResult::mapped(tx, sel))
}

/// Headings to act on for promote/demote: just the one at the cursor, or it plus descendants.
fn affected(doc: &Document, sel: Selection, subtree: bool) -> Option<Vec<HeadingInfo>> {
    let headings = doc.tree().headings();
    let i = heading_at(doc, &headings, sel.head)?;
    let root = headings[i].range.clone();
    Some(if subtree {
        headings
            .into_iter()
            .filter(|h| root.start <= h.range.start && h.range.start < root.end)
            .collect()
    } else {
        vec![headings[i].clone()]
    })
}

/// Make the heading (or subtree) one level shallower. Does nothing at level 1.
pub fn promote(doc: &Document, sel: Selection, subtree: bool) -> Option<OpResult> {
    let hs = affected(doc, sel, subtree)?;
    if hs[0].level <= 1 {
        return None;
    }
    let tx = hs.iter().fold(Transaction::new(), |tx, h| {
        tx.with(h.range.start..h.range.start + 1, "")
    });
    Some(OpResult::mapped(tx, sel))
}

/// Make the heading (or subtree) one level deeper.
pub fn demote(doc: &Document, sel: Selection, subtree: bool) -> Option<OpResult> {
    let hs = affected(doc, sel, subtree)?;
    let tx = hs.iter().fold(Transaction::new(), |tx, h| {
        tx.with(h.range.start..h.range.start, "*")
    });
    Some(OpResult::mapped(tx, sel))
}

/// Swap the subtree at the cursor with its previous (`up`) or next sibling.
pub fn move_subtree(doc: &Document, sel: Selection, up: bool) -> Option<OpResult> {
    let headings = doc.tree().headings();
    let h = &headings[heading_at(doc, &headings, sel.head)?];
    let sibling = if up {
        headings
            .iter()
            .find(|s| s.level == h.level && s.range.end == h.range.start)
    } else {
        headings
            .iter()
            .find(|s| s.level == h.level && s.range.start == h.range.end)
    }?;
    let (first, second) = if up { (sibling, h) } else { (h, sibling) };
    let (a, b) = swap_chunks(doc.text(), first.range.clone(), second.range.clone());
    let start = first.range.start;
    let tx = Transaction::replace(start..second.range.end, format!("{b}{a}"));

    // Keep the cursor on the same text inside the moved subtree.
    let shift = |pos: usize| -> usize {
        if (first.range.start..first.range.end).contains(&pos) {
            start + b.len() + (pos - first.range.start)
        } else if (second.range.start..=second.range.end).contains(&pos) {
            start + (pos - second.range.start)
        } else {
            pos
        }
    };
    let selection = Selection {
        anchor: shift(sel.anchor).min(start + a.len() + b.len()),
        head: shift(sel.head).min(start + a.len() + b.len()),
    };
    Some(OpResult { tx, selection })
}

/// Text of two adjacent chunks, ready to be placed as `b` then `a`.
///
/// If `b` is the end of a file without a trailing newline, the newline moves from `a` to `b`
/// so lines don't merge and the file keeps its no-trailing-newline style.
fn swap_chunks(text: &str, a: Range<usize>, b: Range<usize>) -> (String, String) {
    let mut a = text[a].to_string();
    let mut b = text[b].to_string();
    if !b.ends_with('\n') {
        let eol = if a.ends_with("\r\n") { "\r\n" } else { "\n" };
        if let Some(stripped) = a.strip_suffix(eol) {
            a = stripped.to_string();
            b.push_str(eol);
        }
    }
    (a, b)
}

/// Insert a new sibling heading after the subtree at the cursor (or a top-level heading after
/// the current line when outside any heading), and put the cursor in its title.
pub fn insert_heading(doc: &Document, sel: Selection) -> Option<OpResult> {
    let text = doc.text();
    let eol = line_ending(text);
    let headings = doc.tree().headings();
    let (at, insert, cursor_offset) = match heading_at(doc, &headings, sel.head) {
        Some(i) => {
            let h = &headings[i];
            let stars = "*".repeat(h.level);
            if text[h.range.clone()].ends_with('\n') {
                (h.range.end, format!("{stars} {eol}"), h.level + 1)
            } else {
                (
                    h.range.end,
                    format!("{eol}{stars} "),
                    eol.len() + h.level + 1,
                )
            }
        }
        None => {
            let line_end = text[sel.head..]
                .find(['\r', '\n'])
                .map_or(text.len(), |i| sel.head + i);
            let line_start = text[..sel.head].rfind('\n').map_or(0, |i| i + 1);
            if text[line_start..line_end].trim().is_empty() {
                (line_start, "* ".to_string(), 2)
            } else {
                (line_end, format!("{eol}* "), eol.len() + 2)
            }
        }
    };
    Some(OpResult {
        tx: Transaction::replace(at..at, insert),
        selection: Selection::cursor(at + cursor_offset),
    })
}

/// Toggle the checkbox (`[ ]` ↔ `[X]`, `[-]` → `[X]`) on the list item at the cursor's line.
pub fn toggle_checkbox(doc: &Document, sel: Selection) -> Option<OpResult> {
    let text = doc.text();
    let line_start = text[..sel.head].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |i| line_start + i);
    let line = &text[line_start..line_end];
    let box_at = checkbox_offset(line)?;
    let current = line.as_bytes()[box_at + 1];
    let new = if matches!(current, b'X' | b'x') {
        " "
    } else {
        "X"
    };
    let pos = line_start + box_at + 1;
    Some(OpResult::mapped(
        Transaction::replace(pos..pos + 1, new),
        sel,
    ))
}

/// Byte offset of `[` in a list item's checkbox, e.g. `  - [ ] item` → 4.
fn checkbox_offset(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent..];
    let bullet_len = if rest.starts_with(['-', '+']) || (indent > 0 && rest.starts_with('*')) {
        1
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let alpha =
            (digits == 0 && rest.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())) as usize;
        let n = digits + alpha;
        if n == 0 || !rest[n..].starts_with(['.', ')']) {
            return None;
        }
        n + 1
    };
    let after = &rest[bullet_len..];
    let spaces = after.len() - after.trim_start_matches(' ').len();
    if spaces == 0 {
        return None;
    }
    let cb = &after[spaces..];
    let b = cb.as_bytes();
    let valid = b.len() >= 3
        && b[0] == b'['
        && matches!(b[1], b' ' | b'X' | b'x' | b'-')
        && b[2] == b']'
        && (b.len() == 3 || matches!(b[3], b' ' | b'\r'));
    valid.then_some(indent + bullet_len + spaces)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `op` on `input`, where the last `|` marks the cursor; return the output with `|`
    /// re-inserted at the new cursor.
    fn run(input: &str, op: impl Fn(&Document, Selection) -> Option<OpResult>) -> Option<String> {
        let at = input.rfind('|').expect("cursor marker");
        let mut doc_text = input.to_string();
        doc_text.remove(at);
        let doc = Document::new(doc_text.clone());
        let res = op(&doc, Selection::cursor(at))?;
        let mut out = doc_text;
        res.tx.apply(&mut out);
        out.insert(res.selection.head, '|');
        Some(out)
    }

    fn check(input: &str, op: impl Fn(&Document, Selection) -> Option<OpResult>, expected: &str) {
        assert_eq!(
            run(input, op).as_deref(),
            Some(expected),
            "input: {input:?}"
        );
    }

    #[test]
    fn cycle_todo_default_keywords() {
        let fwd = |d: &Document, s| cycle_todo(d, s, true);
        check("* Ti|tle\n", fwd, "* TODO Ti|tle\n");
        check("* TODO Ti|tle\n", fwd, "* DONE Ti|tle\n");
        check("* DONE Ti|tle\n", fwd, "* Ti|tle\n");
        check("* Title\nbo|dy\n", fwd, "* TODO Title\nbo|dy\n");
        check("* |\n", fwd, "* |TODO\n");
        let back = |d: &Document, s| cycle_todo(d, s, false);
        check("* Ti|tle\n", back, "* DONE Ti|tle\n");
        check("* TODO Ti|tle\n", back, "* Ti|tle\n");
    }

    #[test]
    fn cycle_todo_file_keywords() {
        let fwd = |d: &Document, s| cycle_todo(d, s, true);
        let head = "#+TODO: TODO NEXT | DONE CANCELLED\n";
        check(
            &format!("{head}* TODO |x\n"),
            fwd,
            &format!("{head}* NEXT |x\n"),
        );
        check(
            &format!("{head}* DONE |x\n"),
            fwd,
            &format!("{head}* CANCELLED |x\n"),
        );
        check(
            &format!("{head}* CANCELLED |x\n"),
            fwd,
            &format!("{head}* |x\n"),
        );
    }

    #[test]
    fn cycle_todo_outside_heading_does_nothing() {
        assert_eq!(
            run("intro |text\n* h\n", |d, s| cycle_todo(d, s, true)),
            None
        );
    }

    #[test]
    fn promote_and_demote() {
        let demote1 = |d: &Document, s| demote(d, s, false);
        let demote_tree = |d: &Document, s| demote(d, s, true);
        let promote1 = |d: &Document, s| promote(d, s, false);
        let promote_tree = |d: &Document, s| promote(d, s, true);
        check("* a|\n** b\n* c\n", demote1, "** a|\n** b\n* c\n");
        check("* a|\n** b\n* c\n", demote_tree, "** a|\n*** b\n* c\n");
        check("|* a\n", demote1, "|** a\n");
        check("** a|\n*** b\n", promote_tree, "* a|\n** b\n");
        check("** a|\n", promote1, "* a|\n");
        assert_eq!(run("* a|\n", promote1), None);
    }

    #[test]
    fn move_subtree_up_and_down() {
        let up = |d: &Document, s| move_subtree(d, s, true);
        let down = |d: &Document, s| move_subtree(d, s, false);
        check("* a\nA\n* b|\nB\n", up, "* b|\nB\n* a\nA\n");
        check("* a|\nA\n* b\nB\n", down, "* b\nB\n* a|\nA\n");
        // Children travel with their parent.
        check("* a\n** a1\n* b|\n** b1\n", up, "* b|\n** b1\n* a\n** a1\n");
        // Only siblings swap: no moving past the parent.
        assert_eq!(run("* p\n** a|\n* q\n", up), None);
        assert_eq!(run("* p\n** a|\n* q\n", down), None);
        // File without trailing newline keeps that style.
        check("* a\n* b|", up, "* b|\n* a");
        check("* a|\n* b", down, "* b\n* a|");
        // CRLF files.
        check("* a\r\n* b|", up, "* b|\r\n* a");
    }

    #[test]
    fn insert_heading_after_subtree() {
        check(
            "* a|\n** child\n* b\n",
            insert_heading,
            "* a\n** child\n* |\n* b\n",
        );
        check("* a\n** ch|ild\n", insert_heading, "* a\n** child\n** |\n");
        check("* a|", insert_heading, "* a\n* |");
        check(
            "intro| text\n* a\n",
            insert_heading,
            "intro text\n* |\n* a\n",
        );
        check("|", insert_heading, "* |");
    }

    #[test]
    fn toggle_checkboxes() {
        check("- [ ] it|em\n", toggle_checkbox, "- [X] it|em\n");
        check("- [X] it|em\n", toggle_checkbox, "- [ ] it|em\n");
        check("  + [-] it|em\n", toggle_checkbox, "  + [X] it|em\n");
        check("1. [ ] it|em\n", toggle_checkbox, "1. [X] it|em\n");
        check("  * [ ] it|em\n", toggle_checkbox, "  * [X] it|em\n");
        assert_eq!(run("* [ ] heading|\n", toggle_checkbox), None);
        assert_eq!(run("- plain it|em\n", toggle_checkbox), None);
        assert_eq!(run("-[ ] no spa|ce\n", toggle_checkbox), None);
    }
}
