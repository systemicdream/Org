# 0004 — Workspace index: in memory, rebuilt from files

- Status: accepted (Phase 5)
- Date: 2026-11-22

## Context

Phase 5 needs a workspace index for the file tree, quick switcher, full-text search, tags and
`id:` links. The plan suggested SQLite FTS5 or tantivy, stored in the app cache directory.

## Decision

Keep the index in memory (`org-index`): scan every `.org` file under the workspace folder at
startup, keep each file's text and parsed headings, and update single files when the editor saves
or the file watcher reports a change. Search scans the in-memory text.

- Nothing is written to disk, so there is no cache to invalidate or to leak into a synced folder.
- Personal notes folders are typically a few MB; scanning and searching that is milliseconds.
- The `Index` API (scan, update_file, search, find_id, tags, match_files) doesn't expose the
  storage, so a persistent backend can replace it later without changing callers.

## Revisit when

Startup scans of large workspaces (tens of thousands of files) become noticeable, or search needs
ranking beyond substring matching — then add an on-disk FTS index in the platform cache dir.
