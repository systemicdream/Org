//! Rough parse-time measurement on a synthetic large file.
//! Run with: cargo run --release -p org-syntax --example parse_bench
use std::time::Instant;

use org_syntax::OrgTree;

fn synthetic(sections: usize) -> String {
    let mut s = String::from("#+TITLE: Bench\n#+TODO: TODO NEXT | DONE\n\n");
    for i in 0..sections {
        s.push_str(&format!(
            "* TODO [#B] Section {i} with *bold* and [[https://example.com][a link]] :tag{t}:\n\
             SCHEDULED: <2026-11-21 Sat>\n:PROPERTIES:\n:ID: id-{i}\n:END:\n\
             Paragraph text for section {i}, /italic/ and =code= here.\n\
             - [ ] item one\n- [X] item two\n\
             ** Sub {i}\n| a | b |\n|---+---|\n| 1 | 2 |\n\
             #+BEGIN_SRC rust\nfn f() {{}}\n#+END_SRC\n\n",
            t = i % 7
        ));
    }
    s
}

fn main() {
    for sections in [100, 1_000, 5_000] {
        let text = synthetic(sections);
        let lines = text.lines().count();
        let runs = 5;
        let start = Instant::now();
        for _ in 0..runs {
            std::hint::black_box(OrgTree::parse(&text));
        }
        let per = start.elapsed() / runs;
        println!(
            "{lines:>7} lines, {:>8} KB: full parse {per:?}",
            text.len() / 1024
        );
    }
}
