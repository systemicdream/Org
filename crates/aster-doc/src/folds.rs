//! Folded (collapsed) headings. A fold hides everything in a section after its heading line.
//!
//! Folds are view state keyed by heading start offsets, remapped through each change.

use std::collections::BTreeSet;
use std::ops::Range;

use aster_org::HeadingInfo;

use crate::Transaction;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folds {
    starts: BTreeSet<usize>,
}

/// The bytes hidden when `h` is folded: from the line after the heading to the section end.
/// Empty when the section is just the heading line.
pub fn foldable_range(h: &HeadingInfo, text: &str) -> Range<usize> {
    let eol = if text[h.line_end..].starts_with("\r\n") {
        2
    } else {
        usize::from(text[h.line_end..].starts_with('\n'))
    };
    let start = (h.line_end + eol).min(h.range.end);
    start..h.range.end
}

impl Folds {
    pub fn is_folded(&self, heading_start: usize) -> bool {
        self.starts.contains(&heading_start)
    }

    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// Fold or unfold `h`. Headings with nothing to hide can't be folded.
    pub fn set(&mut self, h: &HeadingInfo, text: &str, folded: bool) {
        if folded && !foldable_range(h, text).is_empty() {
            self.starts.insert(h.range.start);
        } else {
            self.starts.remove(&h.range.start);
        }
    }

    pub fn toggle(&mut self, h: &HeadingInfo, text: &str) {
        let folded = self.is_folded(h.range.start);
        self.set(h, text, !folded);
    }

    pub fn fold_all(&mut self, headings: &[HeadingInfo], text: &str) {
        for h in headings {
            self.set(h, text, true);
        }
    }

    pub fn unfold_all(&mut self) {
        self.starts.clear();
    }

    /// Follow a text change, then drop folds whose heading no longer exists.
    pub fn remap(&mut self, tx: &Transaction, headings: &[HeadingInfo], text: &str) {
        let mapped: BTreeSet<usize> = self.starts.iter().map(|&s| tx.map(s, true)).collect();
        self.starts = headings
            .iter()
            .filter(|h| mapped.contains(&h.range.start) && !foldable_range(h, text).is_empty())
            .map(|h| h.range.start)
            .collect();
    }

    /// Merged, sorted byte ranges currently hidden.
    pub fn hidden_ranges(&self, headings: &[HeadingInfo], text: &str) -> Vec<Range<usize>> {
        let mut out: Vec<Range<usize>> = Vec::new();
        for h in headings
            .iter()
            .filter(|h| self.starts.contains(&h.range.start))
        {
            let r = foldable_range(h, text);
            if r.is_empty() {
                continue;
            }
            match out.last_mut() {
                // Nested folds are already covered by their ancestor.
                Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                _ => out.push(r),
            }
        }
        out
    }

    /// Unfold every heading whose hidden text contains `offset`. Returns whether anything changed.
    pub fn reveal(&mut self, offset: usize, headings: &[HeadingInfo], text: &str) -> bool {
        let before = self.starts.len();
        self.starts.retain(|&s| {
            headings
                .iter()
                .find(|h| h.range.start == s)
                .is_none_or(|h| !foldable_range(h, text).contains(&offset))
        });
        self.starts.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    const TEXT: &str = "* a\nbody\n** a1\nx\n* b\n* c\nlast";

    fn h<'a>(doc: &'a Document, title: &str) -> &'a HeadingInfo {
        doc.headings().iter().find(|h| h.title == title).unwrap()
    }

    #[test]
    fn hidden_ranges_cover_section_bodies() {
        let doc = Document::new(TEXT);
        let mut folds = Folds::default();
        folds.set(h(&doc, "a"), doc.text(), true);
        folds.set(h(&doc, "a1"), doc.text(), true); // nested: merged into a's range
        folds.set(h(&doc, "b"), doc.text(), true); // nothing to hide: ignored
        folds.set(h(&doc, "c"), doc.text(), true); // last section without trailing newline
        let ranges = folds.hidden_ranges(doc.headings(), doc.text());
        let hidden: Vec<&str> = ranges.iter().map(|r| &doc.text()[r.clone()]).collect();
        assert_eq!(hidden, ["body\n** a1\nx\n", "last"]);
        assert!(!folds.is_folded(h(&doc, "b").range.start));
    }

    #[test]
    fn folds_follow_edits_and_disappear_with_their_heading() {
        let mut doc = Document::new(TEXT);
        let mut folds = Folds::default();
        folds.set(h(&doc, "b"), doc.text(), false);
        folds.set(h(&doc, "c"), doc.text(), true);
        // Insert text before "* c": the fold moves with it.
        let tx = Transaction::replace(0..0, "intro\n");
        doc.apply(
            tx.clone(),
            Default::default(),
            Default::default(),
            crate::EditOrigin::Other,
        );
        folds.remap(&tx, doc.headings(), doc.text());
        assert!(folds.is_folded(h(&doc, "c").range.start));
        // Delete the "* c" heading line's stars: the fold goes away.
        let c = h(&doc, "c").range.start;
        let tx = Transaction::replace(c..c + 2, "");
        doc.apply(
            tx.clone(),
            Default::default(),
            Default::default(),
            crate::EditOrigin::Other,
        );
        folds.remap(&tx, doc.headings(), doc.text());
        assert!(folds.is_empty());
    }

    #[test]
    fn reveal_unfolds_ancestors_of_offset() {
        let doc = Document::new(TEXT);
        let mut folds = Folds::default();
        folds.fold_all(doc.headings(), doc.text());
        let x = doc.text().find("x\n").unwrap();
        assert!(folds.reveal(x, doc.headings(), doc.text()));
        assert!(!folds.is_folded(h(&doc, "a").range.start));
        assert!(!folds.is_folded(h(&doc, "a1").range.start));
        assert!(folds.is_folded(h(&doc, "c").range.start));
    }
}
