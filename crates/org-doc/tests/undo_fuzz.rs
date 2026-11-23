//! Randomized structural edits over the Org corpus: undoing everything must restore the
//! original bytes exactly, and redoing everything must reproduce the final text.

use std::fs;
use std::path::PathBuf;

use org_doc::meta::{self, DropTarget, Planning};
use org_doc::{Document, EditOrigin, Selection, ops};

struct Lcg(u64);
impl Lcg {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) % n as u64) as usize
    }
}

#[test]
fn random_ops_undo_to_original() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../org-syntax/tests/corpus");
    let mut paths: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    // Sorted, so the random sequence is the same on every platform.
    paths.sort();
    for seed in [0x0AC7u64, 1, 2, 3, 4, 5, 6, 7] {
        let mut rng = Lcg(seed);
        for path in &paths {
            let original = fs::read_to_string(path).unwrap();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let mut doc = Document::new(original.clone());
            let mut applied = 0;

            for _ in 0..300 {
                let len = doc.text().len();
                let mut at = rng.below(len + 1);
                while !doc.text().is_char_boundary(at) {
                    at -= 1;
                }
                let sel = Selection::cursor(at);
                let hs: Vec<usize> = doc.headings().iter().map(|h| h.range.start).collect();
                let pick = |rng: &mut Lcg| (!hs.is_empty()).then(|| hs[rng.below(hs.len())]);
                let date = chrono::NaiveDate::from_ymd_opt(2026, 1 + rng.below(12) as u32, 1);
                let res = match rng.below(14) {
                    8 => pick(&mut rng).and_then(|h| {
                        let before = pick(&mut rng);
                        let level = 1 + rng.below(3);
                        meta::move_subtree_to(&doc, h, DropTarget { before, level })
                    }),
                    9 => pick(&mut rng)
                        .and_then(|h| meta::set_tags(&doc, h, &["x".into(), "y".into()], sel)),
                    10 => pick(&mut rng).and_then(|h| meta::set_tags(&doc, h, &[], sel)),
                    11 => pick(&mut rng)
                        .and_then(|h| meta::set_planning(&doc, h, Planning::Scheduled, date, sel)),
                    12 => pick(&mut rng)
                        .and_then(|h| meta::set_planning(&doc, h, Planning::Deadline, None, sel)),
                    13 => pick(&mut rng).and_then(|h| meta::set_priority(&doc, h, Some('B'), sel)),
                    0 => ops::cycle_todo(&doc, sel, true),
                    1 => ops::cycle_todo(&doc, sel, false),
                    2 => ops::promote(&doc, sel, rng.below(2) == 0),
                    3 => ops::demote(&doc, sel, rng.below(2) == 0),
                    4 => ops::move_subtree(&doc, sel, true),
                    5 => ops::move_subtree(&doc, sel, false),
                    6 => ops::insert_heading(&doc, sel),
                    _ => ops::toggle_checkbox(&doc, sel),
                };
                if let Some(res) = res {
                    let after = res.selection;
                    doc.apply(res.tx, sel, after, EditOrigin::Other);
                    let text = doc.text();
                    assert!(
                        after.head <= text.len() && text.is_char_boundary(after.head),
                        "{name}: bad selection {after:?}"
                    );
                    applied += 1;
                }
            }
            let final_text = doc.text().to_string();
            while doc.undo().is_some() {}
            assert_eq!(
                doc.text(),
                original,
                "{name}: undo did not restore original ({applied} ops)"
            );
            while doc.redo().is_some() {}
            assert_eq!(
                doc.text(),
                final_text,
                "{name}: redo did not reproduce final text"
            );
        }
    }
}
