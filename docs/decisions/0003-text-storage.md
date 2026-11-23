# 0003 — Text storage: `String` now, rope later

- Status: accepted (Phase 1)
- Date: 2026-11-21

## Context

The plan called for a rope (`ropey` or Zed's `rope`) in `org-doc`. A rope makes edits in the
middle of large documents O(log n) instead of O(n).

## Decision

Store the document as a plain `String` behind `org_doc::Document` for now.

Every edit already does O(n) work elsewhere: the full reparse (ADR 0001), recomputing line
starts, and reshaping every line on paint. A rope would not change the cost of an edit until
those are incremental too, while it would complicate the editor (graphemes, UTF-16 offsets for
IME, and slicing all assume `&str`).

All mutation goes through `Transaction`s and `Document::apply`, so swapping the storage later
is contained to `org-doc` plus the editor's read paths.

## Revisit when

Phase 2 adds virtualized rendering and incremental line layout. If profiling then shows edit
cost dominated by string splicing on large files, move to a rope.
