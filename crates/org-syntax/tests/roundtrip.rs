//! Fidelity tests over the corpus in `tests/corpus`.
//!
//! 1. Lossless round-trip: parse → text must equal the input byte-for-byte.
//! 2. Incremental reparse: applying edits through `replace_range` must produce the same tree
//!    as a full parse of the edited text.

use std::fs;
use std::path::PathBuf;

use org_syntax::OrgTree;

fn corpus() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "org"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "corpus is empty");
    files
        .into_iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let text = String::from_utf8(fs::read(&p).unwrap())
                .unwrap_or_else(|_| panic!("{name} is not UTF-8"));
            (name, text)
        })
        .collect()
}

#[test]
fn corpus_round_trips_byte_for_byte() {
    let mut failures = Vec::new();
    for (name, text) in corpus() {
        if OrgTree::parse(&text).text() != text {
            failures.push(name);
        }
    }
    assert!(failures.is_empty(), "round-trip mismatch: {failures:?}");
}

/// Small deterministic PRNG so failures are reproducible without extra dependencies.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn char_boundary(text: &str, mut i: usize) -> usize {
    while !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

const SNIPPETS: &[&str] = &[
    "",
    "x",
    " ",
    "\n",
    "* ",
    "\n* New heading\n",
    "\n** TODO Child :tag:\n",
    "*bold*",
    ":PROPERTIES:\n:ID: x\n:END:\n",
    "#+BEGIN_SRC\n",
    "#+END_SRC\n",
    "| a | b |\n",
    "- [ ] item\n",
    "SCHEDULED: <2026-11-21 Sat>\n",
    "[[link]]",
    "é🎉",
    "\r\n",
];

#[test]
fn incremental_reparse_matches_full_parse() {
    let mut rng = Lcg(0xA57E2);
    let mut failures = Vec::new();

    for (name, original) in corpus() {
        let mut tree = OrgTree::parse(&original);
        let mut expected = original.clone();

        for step in 0..200 {
            let len = expected.len();
            let start = char_boundary(&expected, if len == 0 { 0 } else { rng.below(len + 1) });
            let max_del = (len - start).min(12);
            let end = char_boundary(
                &expected,
                start
                    + if max_del == 0 {
                        0
                    } else {
                        rng.below(max_del + 1)
                    },
            );
            let end = end.max(start);
            let insert = SNIPPETS[rng.below(SNIPPETS.len())];

            expected.replace_range(start..end, insert);
            tree.replace_range(start..end, insert);

            let full = OrgTree::parse(&expected);
            if tree.text() != expected || tree.debug_tree() != full.debug_tree() {
                failures.push(format!(
                    "{name} step {step}: replace {start}..{end} with {insert:?}"
                ));
                break;
            }
        }
    }
    assert!(
        failures.is_empty(),
        "incremental reparse diverged:\n{}",
        failures.join("\n")
    );
}
