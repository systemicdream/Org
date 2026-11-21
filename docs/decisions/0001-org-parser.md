# 0001 — Org parser: `orgize-ng` with full reparse

- Status: accepted (Phase 0)
- Date: 2026-11-21

## Context

Aster needs a lossless Org parser (byte-identical round-trip) that can keep up with typing.
Candidates: `orgize` (PoiScript, unmaintained), `orgize-ng` (maintained fork, same API),
tree-sitter-org (incomplete grammar), or a hand-written parser.

## Findings

Spike code: `crates/aster-org` (tests in `tests/roundtrip.rs`, timing in `examples/parse_bench.rs`).

1. **Round-trip: pass.** All corpus files round-trip byte-for-byte, including CRLF, BOM,
   no trailing newline, empty files, unterminated blocks, and mixed Unicode.
2. **Incremental reparse (`Org::replace_range`): broken.** It silently drops text in common
   edits. Minimal repro: in `"* a\nbody\n"`, inserting `"* "` at offset 4 yields `"* a\n"`
   instead of `"* a\n* body\n"`. Turning a body line into a heading is one of the most common
   outliner edits, so this is disqualifying.
3. **Full parse is fast** (release build, Apple Silicon, synthetic dense Org):

   | Lines  | Size    | Full parse |
   |--------|---------|------------|
   | 1.6k   | 29 KB   | ~0.9 ms    |
   | 16k    | 297 KB  | ~8 ms      |
   | 80k    | 1.5 MB  | ~44 ms     |

4. `orgize` does not read `#+TODO:` from the file itself; Aster builds a `ParseConfig` from
   in-file keyword lines before parsing.

## Decision

- Use `orgize-ng`, pinned exactly, behind `aster_org::OrgTree` so nothing else depends on it.
- `OrgTree::replace_range` does a **full reparse**. Correct by construction; fast enough for
  typical notes files (well under one frame up to ~16k lines).
- The randomized "incremental == full parse" test stays in CI to guard any future
  incremental implementation.

## Revisit when

- Files over ~20k lines feel laggy → reparse only the affected top-level section and splice
  the green tree, or move parsing off the UI thread with debouncing.
- Fidelity bugs pile up in `orgize-ng` → write our own line-oriented parser behind `OrgTree`.
- Consider reporting the `replace_range` bug upstream (szaffarano/orgize).
