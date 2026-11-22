//! Undo/redo history with coalescing of consecutive typing.

use std::time::{Duration, Instant};

use crate::{Selection, Transaction};

/// Why an edit happened; decides whether it merges with the previous undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditOrigin {
    /// Typing characters (merges with adjacent typing).
    Insert,
    /// Backspace / forward delete (merges with adjacent deletes).
    Delete,
    /// Anything else: paste, cut, structural commands. Always its own undo step.
    Other,
}

/// Typing pauses longer than this start a new undo step.
const COALESCE_WINDOW: Duration = Duration::from_millis(1000);

pub(crate) struct Entry {
    /// Applies the change (used for redo).
    pub forward: Transaction,
    /// Reverts the change (used for undo).
    pub inverse: Transaction,
    pub before: Selection,
    pub after: Selection,
    origin: EditOrigin,
    at: Instant,
}

#[derive(Default)]
pub(crate) struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    /// Force the next edit to start a new undo step.
    sealed: bool,
}

impl History {
    pub fn record(
        &mut self,
        forward: Transaction,
        inverse: Transaction,
        before: Selection,
        after: Selection,
        origin: EditOrigin,
        at: Instant,
    ) {
        self.redo.clear();
        let sealed = std::mem::take(&mut self.sealed);
        if !sealed
            && let Some(last) = self.undo.last_mut()
            && let Some((fwd, inv)) = coalesce(last, &forward, &inverse, origin, at)
        {
            last.forward = fwd;
            last.inverse = inv;
            last.after = after;
            last.at = at;
            return;
        }
        self.undo.push(Entry {
            forward,
            inverse,
            before,
            after,
            origin,
            at,
        });
    }

    /// Start a new undo step on the next edit (e.g. after the cursor moves).
    pub fn seal(&mut self) {
        self.sealed = true;
    }

    pub fn pop_undo(&mut self) -> Option<&Entry> {
        let e = self.undo.pop()?;
        self.redo.push(e);
        self.sealed = true;
        self.redo.last()
    }

    pub fn pop_redo(&mut self) -> Option<&Entry> {
        let e = self.redo.pop()?;
        self.undo.push(e);
        self.sealed = true;
        self.undo.last()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// Merge `next` into `last` when both are single-edit typing or deleting runs that touch.
/// Returns the merged (forward, inverse) pair.
fn coalesce(
    last: &Entry,
    next: &Transaction,
    next_inverse: &Transaction,
    origin: EditOrigin,
    at: Instant,
) -> Option<(Transaction, Transaction)> {
    if origin == EditOrigin::Other
        || origin != last.origin
        || at.duration_since(last.at) > COALESCE_WINDOW
    {
        return None;
    }
    let [a] = last.forward.edits() else {
        return None;
    };
    let [b] = next.edits() else { return None };
    let [a_inv] = last.inverse.edits() else {
        return None;
    };
    let [b_inv] = next_inverse.edits() else {
        return None;
    };

    match origin {
        EditOrigin::Insert => {
            // `b` inserts exactly where `a`'s inserted text ends; break on newlines so each line
            // is its own undo step.
            let a_end = a.range.start + a.text.len();
            if !(b.range.is_empty() && b.range.start == a_end) || b.text.contains('\n') {
                return None;
            }
            let fwd = Transaction::replace(a.range.clone(), format!("{}{}", a.text, b.text));
            let inv = Transaction::replace(
                a_inv.range.start..a_inv.range.end + b.text.len(),
                a_inv.text.clone(),
            );
            Some((fwd, inv))
        }
        EditOrigin::Delete => {
            if !(a.text.is_empty() && b.text.is_empty()) {
                return None;
            }
            let (start, removed) = if b.range.end == a.range.start {
                // Backspace run: `b` deletes just before `a`.
                (b.range.start, format!("{}{}", b_inv.text, a_inv.text))
            } else if b.range.start == a.range.start {
                // Forward-delete run: `b` deletes what now follows the cursor.
                (a.range.start, format!("{}{}", a_inv.text, b_inv.text))
            } else {
                return None;
            };
            let fwd = Transaction::replace(start..start + removed.len(), "");
            let inv = Transaction::replace(start..start, removed);
            Some((fwd, inv))
        }
        EditOrigin::Other => None,
    }
}
