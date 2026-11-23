//! Editing heading metadata: TODO state, priority, tags, planning dates, and moving subtrees
//! anywhere in the outline. Headings are identified by their start offset.

use std::ops::Range;

use aster_org::HeadingInfo;
use chrono::NaiveDate;

use crate::dates;
use crate::ops::OpResult;
use crate::{Document, Selection, Transaction};

/// The heading starting at `start`.
pub fn heading(doc: &Document, start: usize) -> Option<&HeadingInfo> {
    doc.headings().iter().find(|h| h.range.start == start)
}

/// All TODO states in order, with whether each counts as done.
pub fn todo_states(doc: &Document) -> Vec<(String, bool)> {
    let (todo, done) = doc.tree().todo_keywords();
    todo.iter()
        .map(|t| (t.clone(), false))
        .chain(done.iter().map(|d| (d.clone(), true)))
        .collect()
}

fn line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Set (or clear) the TODO keyword.
pub fn set_todo(
    doc: &Document,
    start: usize,
    state: Option<&str>,
    sel: Selection,
) -> Option<OpResult> {
    let h = heading(doc, start)?;
    let tx = match (&h.todo_range, state) {
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

/// Byte range of a `[#X]` priority cookie (plus one trailing space, if any).
pub fn priority_range(doc: &Document, h: &HeadingInfo) -> Option<Range<usize>> {
    let line = &doc.text()[h.title_start..h.line_end];
    let b = line.as_bytes();
    if b.len() >= 4 && b[0] == b'[' && b[1] == b'#' && b[3] == b']' {
        let space = usize::from(b.get(4) == Some(&b' '));
        Some(h.title_start..h.title_start + 4 + space)
    } else {
        None
    }
}

/// Set (or clear) the priority, e.g. `Some('A')`.
pub fn set_priority(
    doc: &Document,
    start: usize,
    priority: Option<char>,
    sel: Selection,
) -> Option<OpResult> {
    let h = heading(doc, start)?;
    let existing = priority_range(doc, h);
    let tx = match (existing, priority) {
        (Some(r), Some(p)) => Transaction::replace(r.start..r.start + 4, format!("[#{p}]")),
        (Some(r), None) => Transaction::replace(r, ""),
        (None, Some(p)) => {
            let sep = if h.title_start == h.line_end { "" } else { " " };
            Transaction::replace(h.title_start..h.title_start, format!("[#{p}]{sep}"))
        }
        (None, None) => return None,
    };
    Some(OpResult::mapped(tx, sel))
}

/// Byte range of the tag string (`:a:b:`) on a heading line, if any.
pub fn tags_range(doc: &Document, h: &HeadingInfo) -> Option<Range<usize>> {
    if h.tags.is_empty() {
        return None;
    }
    let line = &doc.text()[h.range.start..h.line_end];
    let trimmed = line.trim_end();
    let start = trimmed.rfind(char::is_whitespace)? + 1;
    trimmed[start..]
        .starts_with(':')
        .then(|| h.range.start + start..h.range.start + trimmed.len())
}

/// Replace the heading's tags. An empty list removes them (and the space before them).
pub fn set_tags(doc: &Document, start: usize, tags: &[String], sel: Selection) -> Option<OpResult> {
    let h = heading(doc, start)?;
    let text = doc.text();
    let tags: Vec<&str> = tags
        .iter()
        .map(|t| t.trim().trim_matches(':'))
        .filter(|t| !t.is_empty())
        .collect();
    let tag_string = (!tags.is_empty()).then(|| format!(":{}:", tags.join(":")));
    let tx = match (tags_range(doc, h), tag_string) {
        (Some(r), Some(s)) => Transaction::replace(r, s),
        (Some(r), None) => {
            // Also remove the whitespace between the title and the tags.
            let before = text[..r.start].trim_end_matches([' ', '\t']).len();
            Transaction::replace(before.max(h.title_start)..r.end, "")
        }
        (None, Some(s)) => {
            let end = h.range.start + text[h.range.start..h.line_end].trim_end().len();
            Transaction::replace(end..h.line_end, format!(" {s}"))
        }
        (None, None) => return None,
    };
    Some(OpResult::mapped(tx, sel))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Planning {
    Scheduled,
    Deadline,
}

impl Planning {
    pub fn keyword(self) -> &'static str {
        match self {
            Planning::Scheduled => "SCHEDULED:",
            Planning::Deadline => "DEADLINE:",
        }
    }
}

/// The planning line right after a heading (SCHEDULED/DEADLINE/CLOSED), as a byte range
/// without its terminator.
fn planning_line(doc: &Document, h: &HeadingInfo) -> Option<Range<usize>> {
    let text = doc.text();
    let start = text[h.line_end..].find('\n').map(|i| h.line_end + i + 1)?;
    if start >= h.range.end {
        return None;
    }
    let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    let end = if text[start..end].ends_with('\r') {
        end - 1
    } else {
        end
    };
    let line = text[start..end].trim_start();
    ["SCHEDULED:", "DEADLINE:", "CLOSED:"]
        .iter()
        .any(|k| line.starts_with(k))
        .then_some(start..end)
}

/// The date currently set for `kind` on a heading.
pub fn planning_date(doc: &Document, start: usize, kind: Planning) -> Option<NaiveDate> {
    let h = heading(doc, start)?;
    let line = planning_line(doc, h)?;
    let text = &doc.text()[line];
    let at = text.find(kind.keyword())? + kind.keyword().len();
    dates::parse_date(text[at..].trim_start())
}

/// Set (or clear) the SCHEDULED or DEADLINE date of a heading, keeping any time/repeater.
pub fn set_planning(
    doc: &Document,
    start: usize,
    kind: Planning,
    date: Option<NaiveDate>,
    sel: Selection,
) -> Option<OpResult> {
    let h = heading(doc, start)?;
    let text = doc.text();
    let kw = kind.keyword();
    let tx = match planning_line(doc, h) {
        Some(line) => {
            let l = &text[line.clone()];
            match l.find(kw) {
                Some(k) => {
                    // Range of "KEYWORD: <timestamp>".
                    let after = k + kw.len();
                    let ws = l[after..].len() - l[after..].trim_start().len();
                    let ts_start = after + ws;
                    let close = match l.as_bytes().get(ts_start) {
                        Some(b'<') => '>',
                        Some(b'[') => ']',
                        _ => return None,
                    };
                    let ts_end = ts_start + l[ts_start..].find(close)? + 1;
                    match date {
                        Some(d) => {
                            let new = dates::with_date(&l[ts_start..ts_end], d)?;
                            Transaction::replace(line.start + ts_start..line.start + ts_end, new)
                        }
                        None => {
                            let rest_empty =
                                l[..k].trim().is_empty() && l[ts_end..].trim().is_empty();
                            if rest_empty {
                                // Remove the whole planning line including its terminator.
                                let eol_end = text[line.end..]
                                    .find('\n')
                                    .map_or(text.len(), |i| line.end + i + 1);
                                Transaction::replace(line.start..eol_end, "")
                            } else {
                                // Remove "KEYWORD: <…>" and one adjacent space.
                                let mut s = line.start + k;
                                let mut e = line.start + ts_end;
                                if text[e..].starts_with(' ') {
                                    e += 1;
                                } else if text[..s].ends_with(' ') {
                                    s -= 1;
                                }
                                Transaction::replace(s..e, "")
                            }
                        }
                    }
                }
                None => {
                    let d = date?;
                    // Org keeps DEADLINE before SCHEDULED; just append for simplicity.
                    let insert = format!(" {kw} {}", dates::active_timestamp(d));
                    Transaction::replace(line.end..line.end, insert)
                }
            }
        }
        None => {
            let d = date?;
            let eol = line_ending(text);
            let at = text[h.line_end..]
                .find('\n')
                .map_or(text.len(), |i| h.line_end + i + 1);
            let insert = if at == text.len() && !text.ends_with('\n') {
                format!("{eol}{kw} {}", dates::active_timestamp(d))
            } else {
                format!("{kw} {}{eol}", dates::active_timestamp(d))
            };
            Transaction::replace(at..at, insert)
        }
    };
    Some(OpResult::mapped(tx, sel))
}

/// Replace the date of the timestamp at `range`.
pub fn set_timestamp_date(
    doc: &Document,
    range: Range<usize>,
    date: NaiveDate,
    sel: Selection,
) -> Option<OpResult> {
    let new = dates::with_date(&doc.text()[range.clone()], date)?;
    Some(OpResult::mapped(Transaction::replace(range, new), sel))
}

/// Where a dragged subtree should go: before the heading starting at `before` (or at the end of
/// the document when `None`), as a heading of `level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropTarget {
    pub before: Option<usize>,
    pub level: usize,
}

/// Move the subtree starting at `start` to `target`, shifting its heading levels so its root
/// becomes `target.level`. Returns `None` for a drop inside the subtree itself or a no-op.
pub fn move_subtree_to(doc: &Document, start: usize, target: DropTarget) -> Option<OpResult> {
    let h = heading(doc, start)?;
    let text = doc.text();
    let eol = line_ending(text);
    let range = h.range.clone();
    let at = target.before.unwrap_or(text.len());
    if at > range.start && at < range.end {
        return None; // into itself
    }
    if (at == range.start || at == range.end) && target.level == h.level {
        return None; // same place
    }

    // The chunk, with heading levels shifted.
    let delta = target.level as isize - h.level as isize;
    let mut chunk = String::new();
    for line in text[range.clone()].split_inclusive('\n') {
        let stars = line.bytes().take_while(|&b| b == b'*').count();
        if stars > 0 && line[stars..].starts_with(' ') {
            let level = (stars as isize + delta).max(1) as usize;
            chunk.push_str(&"*".repeat(level));
            chunk.push_str(&line[stars..]);
        } else {
            chunk.push_str(line);
        }
    }

    // Dropped onto its own position at a different level: just re-level it in place.
    if at == range.start || at == range.end {
        let original = &text[range.clone()];
        let chunk = if original.ends_with('\n') {
            chunk
        } else {
            chunk.trim_end_matches(['\r', '\n']).to_string()
        };
        return Some(OpResult {
            tx: Transaction::replace(range.clone(), chunk),
            selection: Selection::cursor(range.start),
        });
    }

    // Removing the subtree: if it ends the file without a newline, take the newline before it
    // too, so the file doesn't end with a dangling empty line.
    let mut remove = range.clone();
    if !chunk.ends_with('\n') {
        chunk.push_str(eol);
        if remove.start > 0 && text[..remove.start].ends_with('\n') {
            remove.start -= if text[..remove.start].ends_with("\r\n") {
                2
            } else {
                1
            };
        }
    }
    // Inserting at the very end of a file without a trailing newline.
    let text_after_removal_ends_with_nl = if at == text.len() {
        if remove.end == text.len() {
            text[..remove.start].ends_with('\n') || remove.start == 0
        } else {
            text.ends_with('\n')
        }
    } else {
        true
    };
    let insert = if !text_after_removal_ends_with_nl {
        let trimmed = chunk.strip_suffix(eol).unwrap_or(&chunk);
        format!("{eol}{trimmed}")
    } else {
        chunk
    };

    let lead = insert.len() - insert.trim_start_matches(['\r', '\n']).len();
    let tx = if at <= remove.start {
        Transaction::new()
            .with(at..at, insert)
            .with(remove.clone(), "")
    } else {
        Transaction::new()
            .with(remove.clone(), "")
            .with(at..at, insert)
    };
    // Cursor at the moved heading's start.
    let new_start = if at <= remove.start {
        at + lead
    } else {
        at - remove.len() + lead
    };
    Some(OpResult {
        tx,
        selection: Selection::cursor(new_start),
    })
}

/// The subtree at `start` as text with its root moved to `level`, ending with a newline.
pub fn subtree_text(doc: &Document, start: usize, level: usize) -> Option<String> {
    let h = heading(doc, start)?;
    let text = doc.text();
    let eol = line_ending(text);
    let delta = level as isize - h.level as isize;
    let mut chunk = String::new();
    for line in text[h.range.clone()].split_inclusive('\n') {
        let stars = line.bytes().take_while(|&b| b == b'*').count();
        if stars > 0 && line[stars..].starts_with(' ') {
            chunk.push_str(&"*".repeat((stars as isize + delta).max(1) as usize));
            chunk.push_str(&line[stars..]);
        } else {
            chunk.push_str(line);
        }
    }
    if !chunk.ends_with('\n') {
        chunk.push_str(eol);
    }
    Some(chunk)
}

/// Remove the subtree at `start`, leaving the cursor where it was.
pub fn remove_subtree(doc: &Document, start: usize) -> Option<OpResult> {
    let h = heading(doc, start)?;
    let text = doc.text();
    let mut range = h.range.clone();
    // At the end of a file without a trailing newline, also take the newline before it.
    if range.end == text.len() && !text.ends_with('\n') && range.start > 0 {
        range.start -= if text[..range.start].ends_with("\r\n") {
            2
        } else {
            1
        };
    }
    let at = range.start.min(text.len() - range.len());
    Some(OpResult {
        tx: Transaction::replace(range, ""),
        selection: Selection::cursor(at),
    })
}

/// Insert `chunk` (a subtree ending with a newline) at the end of the subtree of `parent`, or
/// at the end of the document when `parent` is `None`.
pub fn insert_subtree(doc: &Document, parent: Option<usize>, chunk: &str) -> Option<OpResult> {
    let text = doc.text();
    let at = match parent {
        Some(p) => heading(doc, p)?.range.end,
        None => text.len(),
    };
    let eol = line_ending(text);
    let needs_newline = at > 0 && !text[..at].ends_with('\n');
    let insert = if needs_newline {
        format!("{eol}{chunk}")
    } else {
        chunk.to_string()
    };
    let lead = if needs_newline { eol.len() } else { 0 };
    Some(OpResult {
        tx: Transaction::replace(at..at, insert),
        selection: Selection::cursor(at + lead),
    })
}

/// Add properties to a subtree chunk's property drawer (creating it after the heading line
/// and any planning line).
pub fn with_properties(chunk: &str, props: &[(&str, String)]) -> String {
    let eol = if chunk.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = chunk.split_inclusive('\n').collect();
    let mut i = 1;
    if lines.get(i).is_some_and(|l| {
        let t = l.trim_start();
        t.starts_with("SCHEDULED:") || t.starts_with("DEADLINE:") || t.starts_with("CLOSED:")
    }) {
        i += 1;
    }
    let new_props: String = props
        .iter()
        .map(|(k, v)| format!(":{k}: {v}{eol}"))
        .collect();
    let mut out = String::new();
    if lines
        .get(i)
        .is_some_and(|l| l.trim().eq_ignore_ascii_case(":properties:"))
    {
        // Existing drawer: add before its :END:.
        let end = lines[i..]
            .iter()
            .position(|l| l.trim().eq_ignore_ascii_case(":end:"))
            .map(|p| i + p);
        match end {
            Some(end) => {
                out.extend(lines[..end].iter().copied());
                out.push_str(&new_props);
                out.extend(lines[end..].iter().copied());
                return out;
            }
            None => return chunk.to_string(),
        }
    }
    out.extend(lines[..i.min(lines.len())].iter().copied());
    out.push_str(&format!(":PROPERTIES:{eol}{new_props}:END:{eol}"));
    out.extend(lines[i.min(lines.len())..].iter().copied());
    out
}

/// Titles of the ancestors of the heading at `start`, outermost first.
pub fn outline_path(doc: &Document, start: usize) -> Vec<String> {
    let Some(h) = heading(doc, start) else {
        return Vec::new();
    };
    doc.headings()
        .iter()
        .filter(|a| {
            a.range.start < h.range.start && a.range.end >= h.range.end && a.level < h.level
        })
        .map(|a| a.title.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(doc: &Document, r: Option<OpResult>) -> String {
        let mut t = doc.text().to_string();
        r.expect("op applies").tx.apply(&mut t);
        t
    }

    fn start_of(doc: &Document, title: &str) -> usize {
        doc.headings()
            .iter()
            .find(|h| h.title == title)
            .unwrap()
            .range
            .start
    }

    #[test]
    fn todo_and_priority() {
        let doc = Document::new("* Task\n");
        let s = Selection::default();
        assert_eq!(
            apply(&doc, set_todo(&doc, 0, Some("TODO"), s)),
            "* TODO Task\n"
        );
        let doc = Document::new("* TODO [#A] Task\n");
        assert_eq!(apply(&doc, set_todo(&doc, 0, None, s)), "* [#A] Task\n");
        assert_eq!(
            apply(&doc, set_priority(&doc, 0, Some('C'), s)),
            "* TODO [#C] Task\n"
        );
        assert_eq!(apply(&doc, set_priority(&doc, 0, None, s)), "* TODO Task\n");
        let doc = Document::new("* TODO Task\n");
        assert_eq!(
            apply(&doc, set_priority(&doc, 0, Some('B'), s)),
            "* TODO [#B] Task\n"
        );
    }

    #[test]
    fn tags() {
        let s = Selection::default();
        let doc = Document::new("* Task   \n");
        let t = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            apply(&doc, set_tags(&doc, 0, &t(&["work"]), s)),
            "* Task :work:\n"
        );
        let doc = Document::new("* Task          :a:b:\n");
        assert_eq!(
            apply(&doc, set_tags(&doc, 0, &t(&["a", "c"]), s)),
            "* Task          :a:c:\n"
        );
        assert_eq!(apply(&doc, set_tags(&doc, 0, &[], s)), "* Task\n");
    }

    #[test]
    fn planning() {
        let s = Selection::default();
        let d = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        let doc = Document::new("* Task\nbody\n");
        assert_eq!(
            apply(&doc, set_planning(&doc, 0, Planning::Scheduled, Some(d), s)),
            "* Task\nSCHEDULED: <2026-11-23 Mon>\nbody\n"
        );
        let doc = Document::new("* Task\nSCHEDULED: <2026-01-01 Thu 09:00 +1w>\nbody\n");
        assert_eq!(
            planning_date(&doc, 0, Planning::Scheduled),
            NaiveDate::from_ymd_opt(2026, 1, 1)
        );
        assert_eq!(
            apply(&doc, set_planning(&doc, 0, Planning::Scheduled, Some(d), s)),
            "* Task\nSCHEDULED: <2026-11-23 Mon 09:00 +1w>\nbody\n"
        );
        assert_eq!(
            apply(&doc, set_planning(&doc, 0, Planning::Deadline, Some(d), s)),
            "* Task\nSCHEDULED: <2026-01-01 Thu 09:00 +1w> DEADLINE: <2026-11-23 Mon>\nbody\n"
        );
        assert_eq!(
            apply(&doc, set_planning(&doc, 0, Planning::Scheduled, None, s)),
            "* Task\nbody\n"
        );
        let doc = Document::new("* T\nDEADLINE: <2026-01-02 Fri> SCHEDULED: <2026-01-01 Thu>\n");
        assert_eq!(
            apply(&doc, set_planning(&doc, 0, Planning::Deadline, None, s)),
            "* T\nSCHEDULED: <2026-01-01 Thu>\n"
        );
        let doc = Document::new("* Last");
        assert_eq!(
            apply(&doc, set_planning(&doc, 0, Planning::Deadline, Some(d), s)),
            "* Last\nDEADLINE: <2026-11-23 Mon>"
        );
    }

    #[test]
    fn move_subtrees() {
        let doc = Document::new("* a\n** a1\n* b\nB\n* c\n");
        let b = start_of(&doc, "b");
        let a = start_of(&doc, "a");
        let c = start_of(&doc, "c");
        // b before a, same level.
        let r = move_subtree_to(
            &doc,
            b,
            DropTarget {
                before: Some(a),
                level: 1,
            },
        );
        assert_eq!(apply(&doc, r.clone()), "* b\nB\n* a\n** a1\n* c\n");
        assert_eq!(r.unwrap().selection.head, 0);
        // a (with child) to the end, as level 2 under c.
        let r = move_subtree_to(
            &doc,
            a,
            DropTarget {
                before: None,
                level: 2,
            },
        );
        assert_eq!(apply(&doc, r), "* b\nB\n* c\n** a\n*** a1\n");
        // b into a's children (before c is fine too).
        let r = move_subtree_to(
            &doc,
            c,
            DropTarget {
                before: Some(b),
                level: 2,
            },
        );
        assert_eq!(apply(&doc, r), "* a\n** a1\n** c\n* b\nB\n");
        // Dropping inside itself or in place is refused.
        let a1 = start_of(&doc, "a1");
        assert!(
            move_subtree_to(
                &doc,
                a,
                DropTarget {
                    before: Some(a1),
                    level: 1
                }
            )
            .is_none()
        );
        assert!(
            move_subtree_to(
                &doc,
                b,
                DropTarget {
                    before: Some(b),
                    level: 1
                }
            )
            .is_none()
        );
    }

    #[test]
    fn subtree_extract_insert_and_properties() {
        let doc = Document::new("* a\n** a1\nbody\n* b");
        let a1 = start_of(&doc, "a1");
        assert_eq!(subtree_text(&doc, a1, 1).unwrap(), "* a1\nbody\n");
        let b = start_of(&doc, "b");
        assert_eq!(subtree_text(&doc, b, 2).unwrap(), "** b\n");
        assert_eq!(apply(&doc, remove_subtree(&doc, b)), "* a\n** a1\nbody");
        assert_eq!(apply(&doc, remove_subtree(&doc, a1)), "* a\n* b");
        // Insert under a parent (at the end of its subtree) or at the end of the file.
        let a = start_of(&doc, "a");
        assert_eq!(
            apply(&doc, insert_subtree(&doc, Some(a), "** new\n")),
            "* a\n** a1\nbody\n** new\n* b"
        );
        assert_eq!(
            apply(&doc, insert_subtree(&doc, None, "* new\n")),
            "* a\n** a1\nbody\n* b\n* new\n"
        );
        assert_eq!(outline_path(&doc, a1), ["a"]);

        let p = |c: &str| with_properties(c, &[("ARCHIVE_TIME", "[2026-11-22 Sun 10:00]".into())]);
        assert_eq!(
            p("* x\nbody\n"),
            "* x\n:PROPERTIES:\n:ARCHIVE_TIME: [2026-11-22 Sun 10:00]\n:END:\nbody\n"
        );
        assert_eq!(
            p("* x\nSCHEDULED: <2026-11-22 Sun>\n:PROPERTIES:\n:ID: 1\n:END:\n"),
            "* x\nSCHEDULED: <2026-11-22 Sun>\n:PROPERTIES:\n:ID: 1\n:ARCHIVE_TIME: [2026-11-22 Sun 10:00]\n:END:\n"
        );
    }

    #[test]
    fn relevel_in_place() {
        // Last subtree, no trailing newline, dropped on itself one level deeper.
        let doc = Document::new("* a\n* b\nB");
        let b = start_of(&doc, "b");
        let r = move_subtree_to(
            &doc,
            b,
            DropTarget {
                before: Some(b),
                level: 2,
            },
        );
        assert_eq!(apply(&doc, r), "* a\n** b\nB");
        let r = move_subtree_to(
            &doc,
            b,
            DropTarget {
                before: None,
                level: 2,
            },
        );
        assert_eq!(apply(&doc, r), "* a\n** b\nB");
        let doc = Document::new("* a\n** a1\n* b\n");
        let r = move_subtree_to(
            &doc,
            0,
            DropTarget {
                before: Some(0),
                level: 2,
            },
        );
        assert_eq!(apply(&doc, r), "** a\n*** a1\n* b\n");
    }

    #[test]
    fn move_with_no_trailing_newline() {
        let doc = Document::new("* a\n* b");
        let b = start_of(&doc, "b");
        let r = move_subtree_to(
            &doc,
            b,
            DropTarget {
                before: Some(0),
                level: 1,
            },
        );
        assert_eq!(apply(&doc, r), "* b\n* a");
        let r = move_subtree_to(
            &doc,
            0,
            DropTarget {
                before: None,
                level: 1,
            },
        );
        assert_eq!(apply(&doc, r), "* b\n* a");
    }
}
