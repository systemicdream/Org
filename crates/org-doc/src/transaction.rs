//! Transactions: a batch of non-overlapping text replacements applied atomically.

use std::ops::Range;

/// One replacement, in the coordinates of the document *before* the transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

/// A set of edits applied together, forming a single undo step.
///
/// Edits are kept sorted by position and must not overlap; all ranges refer to the document
/// before any of them is applied.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transaction {
    edits: Vec<Edit>,
}

impl Transaction {
    pub fn new() -> Self {
        Self::default()
    }

    /// A transaction with a single replacement.
    pub fn replace(range: Range<usize>, text: impl Into<String>) -> Self {
        Self::new().with(range, text)
    }

    /// Add a replacement. Panics if it overlaps an existing edit (a programming error).
    pub fn with(mut self, range: Range<usize>, text: impl Into<String>) -> Self {
        assert!(range.start <= range.end, "inverted range {range:?}");
        // An insertion (empty range) sorts before a replacement starting at the same offset,
        // so "insert here, then delete what follows" is a valid pair.
        let i = self.edits.partition_point(|e| {
            e.range.start < range.start
                || (e.range.start == range.start && e.range.is_empty() && !range.is_empty())
        });
        let ok_before = i == 0 || self.edits[i - 1].range.end <= range.start;
        let ok_after = self.edits.get(i).is_none_or(|e| range.end <= e.range.start);
        assert!(ok_before && ok_after, "overlapping edit {range:?}");
        self.edits.insert(
            i,
            Edit {
                range,
                text: text.into(),
            },
        );
        self
    }

    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }

    pub fn is_empty(&self) -> bool {
        self.edits
            .iter()
            .all(|e| e.range.is_empty() && e.text.is_empty())
    }

    /// Apply to `text`, returning the inverse transaction (which undoes this one).
    ///
    /// Panics if any range is out of bounds or not on a char boundary.
    pub fn apply(&self, text: &mut String) -> Transaction {
        let mut inverse = Vec::with_capacity(self.edits.len());
        let mut delta: isize = 0;
        for e in &self.edits {
            let start = (e.range.start as isize + delta) as usize;
            inverse.push(Edit {
                range: start..start + e.text.len(),
                text: text[e.range.clone()].to_string(),
            });
            delta += e.text.len() as isize - e.range.len() as isize;
        }
        for e in self.edits.iter().rev() {
            text.replace_range(e.range.clone(), &e.text);
        }
        Transaction { edits: inverse }
    }

    /// Map a position in the old document to the new one.
    ///
    /// Positions inside a replaced range move to its end, so a cursor in deleted text lands
    /// after the replacement. With `before_inserts`, a position exactly at an insertion point
    /// stays before the inserted text; otherwise it moves after it.
    pub fn map(&self, pos: usize, before_inserts: bool) -> usize {
        let mut delta: isize = 0;
        for e in &self.edits {
            if pos < e.range.start || (pos == e.range.start && before_inserts) {
                break;
            }
            if pos < e.range.end || pos == e.range.start {
                // Inside the replaced range, or at an insertion point.
                return (e.range.start as isize + delta) as usize + e.text.len();
            }
            delta += e.text.len() as isize - e.range.len() as isize;
        }
        (pos as isize + delta) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_and_invert() {
        let mut text = "hello world".to_string();
        let tx = Transaction::new().with(6..11, "there").with(0..0, ">> ");
        let inverse = tx.apply(&mut text);
        assert_eq!(text, ">> hello there");
        inverse.apply(&mut text);
        assert_eq!(text, "hello world");
    }

    #[test]
    fn map_positions() {
        let tx = Transaction::new().with(2..4, "XYZ").with(6..6, "!");
        // "ab|cd|ef" -> "abXYZef!"... positions:
        assert_eq!(tx.map(0, false), 0);
        assert_eq!(tx.map(3, false), 5); // inside replaced range -> end of replacement
        assert_eq!(tx.map(5, false), 6);
        assert_eq!(tx.map(6, true), 7);
        assert_eq!(tx.map(6, false), 8);
    }

    #[test]
    fn insert_before_replacement_at_same_offset() {
        for tx in [
            Transaction::new().with(0..0, "X").with(0..2, ""),
            Transaction::new().with(0..2, "").with(0..0, "X"),
        ] {
            let mut text = "abcd".to_string();
            let inverse = tx.apply(&mut text);
            assert_eq!(text, "Xcd");
            inverse.apply(&mut text);
            assert_eq!(text, "abcd");
        }
    }

    #[test]
    #[should_panic(expected = "overlapping")]
    fn overlapping_edits_panic() {
        let _ = Transaction::new().with(0..3, "a").with(2..4, "b");
    }
}
