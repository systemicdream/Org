//! Randomized structural edits over the Org corpus: undoing everything must restore the
//! original bytes exactly, and redoing everything must reproduce the final text.

use std::fs;
use std::path::PathBuf;

use aster_doc::{Document, EditOrigin, Selection, ops};

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
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../aster-org/tests/corpus");
    let mut rng = Lcg(0x0AC7);
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let original = fs::read_to_string(&path).unwrap();
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
            let res = match rng.below(8) {
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
