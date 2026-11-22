//! Org mode parsing for Aster.
//!
//! This crate is the only place that knows which parser backs Aster. Everything else talks to
//! [`OrgTree`], so the backend (currently `orgize`) can be swapped without touching the editor.
//!
//! Invariant: an [`OrgTree`] is *lossless* — [`OrgTree::text`] always returns exactly the bytes
//! that were parsed, plus any edits applied through [`OrgTree::replace_range`].

mod decorations;

use std::ops::Range;

use orgize::rowan::ast::AstNode;
use orgize::{Org, ParseConfig, ast::Headline};

pub use decorations::{BlockKind, DecoKind, Decoration};

/// A parsed Org document backed by a lossless syntax tree.
pub struct OrgTree {
    org: Org,
}

/// A flattened view of a heading, enough for outlines and quick inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingInfo {
    pub level: usize,
    pub todo: Option<String>,
    /// True when `todo` is a "done" keyword (e.g. `DONE`, or anything after `|` in `#+TODO:`).
    pub done: bool,
    pub priority: Option<String>,
    pub title: String,
    pub tags: Vec<String>,
    /// Byte range of the whole section (heading line through its last descendant).
    pub range: Range<usize>,
    /// End of the heading line, excluding its line terminator.
    pub line_end: usize,
    /// Byte range of the TODO keyword, if any.
    pub todo_range: Option<Range<usize>>,
    /// Where the title (after stars and TODO keyword) starts; a TODO keyword is inserted here.
    pub title_start: usize,
}

impl OrgTree {
    pub fn parse(text: &str) -> Self {
        Self {
            org: parse_config(text).parse(text),
        }
    }

    /// The full document text, reconstructed from the syntax tree.
    pub fn text(&self) -> String {
        self.org.document().syntax().to_string()
    }

    /// Replace `range` (byte offsets) with `new_text` and reparse.
    ///
    /// This deliberately does a full reparse. `orgize`'s own incremental `replace_range` loses
    /// text in common cases (e.g. typing `* ` at the start of a body line drops the section body;
    /// see `docs/decisions/0001-org-parser.md`). Full reparse is correct by construction and fast
    /// enough for now; section-level incremental parsing can come later behind this same API.
    pub fn replace_range(&mut self, range: Range<usize>, new_text: &str) {
        let mut text = self.text();
        text.replace_range(range, new_text);
        self.org = parse_config(&text).parse(&text);
    }

    /// Debug dump of the syntax tree; useful for comparing incremental vs. full parses.
    pub fn debug_tree(&self) -> String {
        format!("{:#?}", self.org.document().syntax())
    }

    /// Live-preview decorations (markup to hide, replace, or style), sorted by start.
    pub fn decorations(&self) -> Vec<Decoration> {
        decorations::collect(&decorations::root_of(&self.org))
    }

    /// TODO keywords in effect: (not-done states, done states), in cycle order.
    pub fn todo_keywords(&self) -> (&[String], &[String]) {
        let (todo, done) = &self.org.config().todo_keywords;
        (todo, done)
    }

    /// All headings in document order.
    pub fn headings(&self) -> Vec<HeadingInfo> {
        let text = self.text();
        self.org
            .document()
            .syntax()
            .descendants()
            .filter_map(Headline::cast)
            .map(|h| {
                let r = h.syntax().text_range();
                let start = usize::from(r.start());
                let level = h.level();
                let line_end = text[start..].find('\n').map_or(text.len(), |i| start + i);
                let line_end = if text[start..line_end].ends_with('\r') {
                    line_end - 1
                } else {
                    line_end
                };
                let after_stars = start + level;
                let rest = &text[after_stars..line_end];
                let keyword_start = after_stars + (rest.len() - rest.trim_start().len());
                let todo = h.todo_keyword().map(|t| t.to_string());
                let todo_range = todo
                    .as_ref()
                    .filter(|t| text[keyword_start..line_end].starts_with(t.as_str()))
                    .map(|t| keyword_start..keyword_start + t.len());
                let title_start = match &todo_range {
                    Some(r) => {
                        let rest = &text[r.end..line_end];
                        r.end + (rest.len() - rest.trim_start().len())
                    }
                    None => keyword_start,
                };
                HeadingInfo {
                    level,
                    todo,
                    done: h.is_done(),
                    priority: h.priority().map(|t| t.to_string()),
                    title: h.title_raw().trim().to_string(),
                    tags: h.tags().map(|t| t.to_string()).collect(),
                    range: start..usize::from(r.end()),
                    line_end,
                    todo_range,
                    title_start,
                }
            })
            .collect()
    }
}

/// Build a parse config from in-file settings (`#+TODO:`, `#+SEQ_TODO:`, `#+TYP_TODO:`).
fn parse_config(text: &str) -> ParseConfig {
    let mut todo = Vec::new();
    let mut done = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if !["#+todo", "#+seq_todo", "#+typ_todo"]
            .iter()
            .any(|k| key.eq_ignore_ascii_case(k))
        {
            continue;
        }
        // Strip fast-access keys like `TODO(t)` or `DONE(d!)`.
        let words: Vec<String> = value
            .split_whitespace()
            .map(|w| w.split('(').next().unwrap_or(w).to_string())
            .collect();
        match words.iter().position(|w| w == "|") {
            Some(bar) => {
                todo.extend_from_slice(&words[..bar]);
                done.extend_from_slice(&words[bar + 1..]);
            }
            // Without `|`, Org treats the last keyword as the done state.
            None => {
                if let Some((last, rest)) = words.split_last() {
                    todo.extend_from_slice(rest);
                    done.push(last.clone());
                }
            }
        }
    }
    if todo.is_empty() && done.is_empty() {
        ParseConfig::default()
    } else {
        ParseConfig {
            todo_keywords: (todo, done),
            ..ParseConfig::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_are_extracted() {
        let tree = OrgTree::parse("* TODO [#A] Buy milk :home:errand:\nbody\n** Sub\n* Next\n");
        let hs = tree.headings();
        assert_eq!(hs.len(), 3);
        assert_eq!(hs[0].level, 1);
        assert_eq!(hs[0].todo.as_deref(), Some("TODO"));
        assert_eq!(hs[0].priority.as_deref(), Some("A"));
        assert_eq!(hs[0].title, "Buy milk");
        assert_eq!(hs[0].tags, vec!["home", "errand"]);
        assert_eq!(hs[1].level, 2);
        assert_eq!(hs[2].title, "Next");
    }

    /// Regression: orgize's incremental `replace_range` dropped the body here.
    #[test]
    fn typing_heading_stars_on_body_line_keeps_text() {
        let mut tree = OrgTree::parse("* a\nbody\n");
        tree.replace_range(4..4, "* ");
        assert_eq!(tree.text(), "* a\n* body\n");
        assert_eq!(tree.headings().len(), 2);
    }

    #[test]
    fn in_file_todo_keywords_are_respected() {
        let text = "#+TODO: TODO(t) NEXT | DONE(d!) CANCELLED\n* NEXT [#A] Go\n* CANCELLED Nope\n";
        let hs = OrgTree::parse(text).headings();
        assert_eq!(hs[0].todo.as_deref(), Some("NEXT"));
        assert_eq!(hs[0].priority.as_deref(), Some("A"));
        assert!(!hs[0].done);
        assert_eq!(hs[1].todo.as_deref(), Some("CANCELLED"));
        assert!(hs[1].done);
    }

    #[test]
    fn heading_positions() {
        let text = "* TODO Title\r\nbody\n\n** Child\n* Next";
        let hs = OrgTree::parse(text).headings();
        assert_eq!(&text[hs[0].todo_range.clone().unwrap()], "TODO");
        assert_eq!(&text[hs[0].title_start..hs[0].line_end], "Title");
        // Sections include blank lines and descendants, up to the next sibling.
        assert_eq!(
            &text[hs[0].range.clone()],
            "* TODO Title\r\nbody\n\n** Child\n"
        );
        assert_eq!(&text[hs[2].range.clone()], "* Next");
        assert_eq!(hs[2].title_start, hs[2].range.start + 2);
        assert_eq!(hs[2].line_end, text.len());
    }
}
