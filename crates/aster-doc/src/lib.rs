//! The document model: text + parsed Org tree + undo history.
//!
//! All changes go through [`Document::apply`] as a [`Transaction`], so every change is
//! undoable, the tree is always in sync with the text, and structural commands (see [`ops`])
//! are testable without any UI.

mod history;
pub mod ops;
mod transaction;

use std::ops::Range;
use std::time::Instant;

use aster_org::OrgTree;
use history::History;

pub use history::EditOrigin;
pub use transaction::{Edit, Transaction};

/// A selection as byte offsets. `head` is where the cursor is; `anchor` is the other end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn cursor(at: usize) -> Self {
        Self {
            anchor: at,
            head: at,
        }
    }

    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Map through a transaction (cursor-like ends move after inserted text).
    pub fn map(&self, tx: &Transaction) -> Self {
        Self {
            anchor: tx.map(self.anchor, false),
            head: tx.map(self.head, false),
        }
    }
}

pub struct Document {
    text: String,
    tree: OrgTree,
    history: History,
    version: u64,
}

impl Document {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            tree: OrgTree::parse(&text),
            text,
            history: History::default(),
            version: 0,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn tree(&self) -> &OrgTree {
        &self.tree
    }

    /// Increments on every change (including undo/redo).
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Apply `tx` as one undoable step. `before`/`after` are the selections to restore on
    /// undo/redo.
    pub fn apply(
        &mut self,
        tx: Transaction,
        before: Selection,
        after: Selection,
        origin: EditOrigin,
    ) {
        self.apply_at(tx, before, after, origin, Instant::now());
    }

    pub(crate) fn apply_at(
        &mut self,
        tx: Transaction,
        before: Selection,
        after: Selection,
        origin: EditOrigin,
        at: Instant,
    ) {
        if tx.is_empty() {
            return;
        }
        let inverse = tx.apply(&mut self.text);
        self.reparse();
        self.history.record(tx, inverse, before, after, origin, at);
    }

    /// Make the next edit start a new undo step (call when the cursor jumps elsewhere).
    pub fn seal_undo_group(&mut self) {
        self.history.seal();
    }

    /// Undo the last step; returns the selection to restore.
    pub fn undo(&mut self) -> Option<Selection> {
        let entry = self.history.pop_undo()?;
        let (tx, sel) = (entry.inverse.clone(), entry.before);
        tx.apply(&mut self.text);
        self.reparse();
        Some(sel)
    }

    /// Redo the last undone step; returns the selection to restore.
    pub fn redo(&mut self) -> Option<Selection> {
        let entry = self.history.pop_redo()?;
        let (tx, sel) = (entry.forward.clone(), entry.after);
        tx.apply(&mut self.text);
        self.reparse();
        Some(sel)
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    fn reparse(&mut self) {
        // Full reparse; see docs/decisions/0001-org-parser.md.
        self.tree = OrgTree::parse(&self.text);
        self.version += 1;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn type_str(doc: &mut Document, at: &mut usize, s: &str, t: Instant) {
        for ch in s.chars() {
            let tx = Transaction::replace(*at..*at, ch.to_string());
            let before = Selection::cursor(*at);
            *at += ch.len_utf8();
            doc.apply_at(tx, before, Selection::cursor(*at), EditOrigin::Insert, t);
        }
    }

    #[test]
    fn typing_coalesces_into_one_undo_step() {
        let t = Instant::now();
        let mut doc = Document::new("* a\n");
        let mut at = 4;
        type_str(&mut doc, &mut at, "hello", t);
        assert_eq!(doc.text(), "* a\nhello");
        assert_eq!(doc.undo(), Some(Selection::cursor(4)));
        assert_eq!(doc.text(), "* a\n");
        assert_eq!(doc.redo(), Some(Selection::cursor(9)));
        assert_eq!(doc.text(), "* a\nhello");
    }

    #[test]
    fn pauses_newlines_and_seals_split_undo_steps() {
        let t = Instant::now();
        let mut doc = Document::new("");
        let mut at = 0;
        type_str(&mut doc, &mut at, "ab", t);
        type_str(&mut doc, &mut at, "cd", t + Duration::from_secs(2));
        type_str(&mut doc, &mut at, "\nef", t + Duration::from_secs(2));
        doc.seal_undo_group();
        type_str(&mut doc, &mut at, "gh", t + Duration::from_secs(2));
        assert_eq!(doc.text(), "abcd\nefgh");
        let mut states = vec![];
        while doc.undo().is_some() {
            states.push(doc.text().to_string());
        }
        assert_eq!(states, ["abcd\nef", "abcd", "ab", ""]);
    }

    #[test]
    fn backspace_and_delete_runs_coalesce() {
        let t = Instant::now();
        let mut doc = Document::new("abcdef");
        // Backspace twice from 4: removes "d" then "c".
        for at in [4usize, 3] {
            let tx = Transaction::replace(at - 1..at, "");
            doc.apply_at(
                tx,
                Selection::cursor(at),
                Selection::cursor(at - 1),
                EditOrigin::Delete,
                t,
            );
        }
        assert_eq!(doc.text(), "abef");
        // Forward delete twice at the cursor (2): removes "e" then "f". Without a seal this would
        // extend the backspace step (both are adjacent deletes), which is also acceptable.
        doc.seal_undo_group();
        for _ in 0..2 {
            let tx = Transaction::replace(2..3, "");
            doc.apply_at(
                tx,
                Selection::cursor(2),
                Selection::cursor(2),
                EditOrigin::Delete,
                t,
            );
        }
        assert_eq!(doc.text(), "ab");
        doc.undo();
        assert_eq!(doc.text(), "abef");
        doc.undo();
        assert_eq!(doc.text(), "abcdef");
        assert!(!doc.can_undo());
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut doc = Document::new("x");
        doc.apply(
            Transaction::replace(1..1, "y"),
            Selection::cursor(1),
            Selection::cursor(2),
            EditOrigin::Other,
        );
        doc.undo();
        assert!(doc.can_redo());
        doc.apply(
            Transaction::replace(1..1, "z"),
            Selection::cursor(1),
            Selection::cursor(2),
            EditOrigin::Other,
        );
        assert!(!doc.can_redo());
        assert_eq!(doc.text(), "xz");
    }

    #[test]
    fn tree_tracks_text_through_undo() {
        let mut doc = Document::new("* a\nbody\n");
        doc.apply(
            Transaction::replace(4..4, "* "),
            Selection::cursor(4),
            Selection::cursor(6),
            EditOrigin::Other,
        );
        assert_eq!(doc.tree().headings().len(), 2);
        doc.undo();
        assert_eq!(doc.tree().headings().len(), 1);
    }
}
