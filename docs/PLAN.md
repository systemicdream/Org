# Aster — Project Plan

Aster is a modern, cross-platform Org mode client built with [GPUI](https://www.gpui.rs/).
It brings Org's note-taking and organization workflow to people who are interested in
Org mode but put off by Emacs — keeping the Org file format, with a modern, discoverable editor.

---

## 1. Audience

**Primary:** newcomers who like the *idea* of Org mode (plain-text notes, outlines, TODOs,
agenda, capture) but don't want to learn Emacs, Elisp, or a wall of keybindings.

What this means for every design decision:

- **Discoverable over memorized.** Every action is reachable by mouse, menu, slash command,
  or command palette. Keyboard shortcuts are shown next to actions so users learn them gradually.
- **Familiar defaults.** Standard platform shortcuts (Cmd/Ctrl+B, Cmd/Ctrl+S, Cmd/Ctrl+Z, etc.).
  An optional Emacs-style keymap exists for those who want it, but it's never required.
- **Org concepts explained in the UI.** TODO states, tags, properties, scheduled/deadline, and
  the agenda get plain-language labels, tooltips, and empty states that teach.
- **Zero configuration to be useful.** Sensible defaults; settings are a GUI, not a config file.
- **Still real Org.** Files stay standard `.org` so users can later move to (or share with)
  Emacs, Orgzly, Beorg, etc. without lock-in.

**Secondary:** existing Org users who want a nicer front-end. We serve them through file
fidelity and the optional Emacs keymap, but we don't design the UX around them.

---

## 2. Core principles

1. **Plain `.org` files are the only source of truth.** No proprietary database or format.
   Any index/cache is disposable and rebuilt from files.
2. **Lossless round-trip.** Parse → serialize with no edits yields identical bytes. Edits only
   rewrite the regions they touch. No reformatting of untouched text.
3. **Live preview editing.** The buffer is raw Org text, rendered styled: markup (`*bold*`,
   `[[link][desc]]`, drawers) is hidden or de-emphasized and revealed when the cursor enters it.
   Rich widgets (checkboxes, link chips, tables, date pills) are overlays on real text.
4. **Structure is first-class.** Headings are objects you can fold, drag, move, tag, schedule,
   and refile with the mouse or keyboard.
5. **No sync.** Aster reads and writes files in a folder. Users choose their own sync
   (iCloud, Dropbox, Syncthing, Git, …). Aster must behave well when files change underneath it.

---

## 3. Scope

### In scope (v1)

| Area | Features |
|---|---|
| Outline | Headings, folding/cycling, outline sidebar, breadcrumbs, focus/zoom into a subtree |
| Tasks | TODO keywords (incl. custom `#+TODO:`), priorities, checkboxes, progress cookies `[2/5]` / `[40%]` |
| Metadata | Tags (with inheritance, `#+FILETAGS:`), property drawers, `:ID:` generation |
| Dates | Timestamps, `SCHEDULED` / `DEADLINE`, repeaters, date picker |
| Text | Emphasis, lists, quotes, footnotes, horizontal rules, inline images |
| Links | `https:`, `file:`, `id:`, internal `*heading` links, link autocomplete |
| Tables | Render as grid, auto-align, Tab/Enter navigation, add/remove rows & columns |
| Code | Src blocks with tree-sitter syntax highlighting (no execution) |
| Workspace | Open a folder, file tree, quick switcher, full-text search, tag browser |
| Organization | Agenda (day/week, TODO list, tag/property search), capture templates, refile, archive |
| Onboarding | Welcome/tutorial notebook, templates, contextual hints |

### Out of scope (for now)

- Babel / code execution
- Elisp or any Emacs config compatibility
- Export (possible later via optional Pandoc integration)
- Clocking, column view, table formulas (render formulas, don't evaluate)
- Built-in sync
- Mobile (GPUI is desktop-only)
- Windows, for now: target macOS and Linux first; GPUI supports Windows, so it can be added later
- X11: Linux support is Wayland-only (GPUI's X11 backend is compiled out)

### Future candidates

- **Org-roam compatibility** (ID-based linking, backlinks panel, dailies). We generate
  org-id–compatible `:ID:` properties from day one so this can be added without migration.
- Export, clocking, plugin system, Pandoc import.

---

## 4. Architecture

Cargo workspace. Only the UI crates depend on GPUI, keeping the core fast to test and insulated
from GPUI API churn.

```
crates/
  aster-org/      Org parser, lossless syntax tree, serializer. No GPUI. Heavily tested.
  aster-doc/      Document model: rope buffer, edit transactions, undo/redo, incremental
                  reparse, structural ops (promote/demote, move subtree, cycle TODO, …).
  aster-index/    Workspace index: files, headings, tags, IDs, TODOs, timestamps, links,
                  full-text search. File watching. Disposable cache.
  aster-agenda/   Agenda queries and views over the index (pure logic).
  aster-editor/   GPUI editor view: layout, live-preview rendering, input/IME, selection,
                  folding, inline widgets.
  aster-ui/       Theme, design tokens, shared components, icons.
  aster-app/      App shell: window, sidebar, tabs, command palette, agenda view, capture,
                  settings, onboarding, state persistence.
```

### Dependency direction

```
aster-app ──► aster-editor ──► aster-doc ──► aster-org
    │              │
    ├──► aster-ui ◄┘
    ├──► aster-agenda ──► aster-index ──► aster-org
    └──► aster-index
```

### Technology choices

| Concern | Choice | Notes |
|---|---|---|
| UI framework | `gpui` | Pin an exact version / git rev. Zed's source is the de-facto documentation. |
| Components | Evaluate `gpui-component` | Inputs, lists, virtualized tables, dock, popovers; reference for a multi-line editor. |
| Text buffer | `ropey` (or Zed's `rope`/`sum_tree`) | Zed's `editor` crate is too coupled to Zed to reuse. |
| Org parsing | Start with `orgize` 0.10 (rowan, lossless) behind an internal trait | Replace with a hand-written section-incremental parser if fidelity or perf falls short. |
| Code highlighting | `tree-sitter` + grammars | Src blocks only. |
| Index / search | `rusqlite` with FTS5 (alt: `tantivy`) | Stored in app cache dir, never next to user files. |
| File watching | `notify` | Reload clean buffers; conflict UI for dirty buffers. |
| Dates | `jiff` or `chrono` | Repeaters, agenda ranges, time zones. |
| Settings / state | `serde` + TOML/JSON in platform config dir | Edited through the settings UI. |
| Packaging | `cargo-packager` | `.dmg` (signed + notarized), AppImage/`.deb`/Flatpak; `.msi` once Windows is picked up. |
| CI | GitHub Actions on macOS and Linux | From week one. Windows added when it is picked up. |

---

## 5. Editor design

The editor is the product. It deserves the most care and the most time.

### Model

- Rope holds the text; a syntax tree is maintained over it.
- Every change is a **transaction**: apply rope edits → reparse affected section(s)
  (Org is line/section oriented, so reparse from the enclosing heading) → update view.
- Undo/redo operates on transactions, so a structural op ("move subtree up") is one undo step.

### Rendering

- Syntax nodes map to **display spans**: style, hidden ranges, and replacement widgets.
- **Cursor-aware reveal:** markup is hidden except inside the node containing the cursor/selection.
- Virtualized line/block list so large files (10k+ lines) stay smooth.
- Widgets rendered inline: checkboxes, TODO/priority pills, tag chips, link chips,
  date pills, table grids, images, highlighted code blocks.

### Input

- Implement GPUI's `EntityInputHandler` correctly from the start (IME, dead keys, CJK).
- Soft wrap, word/line movement, multi-click selection, drag-and-drop text.

### Newcomer-friendly interactions

- **Slash menu:** `/heading`, `/todo`, `/checklist`, `/table`, `/code`, `/date`, `/link`, `/quote`.
- **Hover/selection toolbar** for formatting.
- **Heading gutter:** fold arrow + drag handle + "⋯" menu (set TODO, priority, tags, schedule,
  deadline, refile, archive, copy link).
- **Clickable metadata:** click a TODO pill to cycle/choose state, a date pill for a date picker,
  a tag chip to filter by tag.
- **Command palette** (Cmd/Ctrl+K or Cmd/Ctrl+Shift+P) for every action, showing shortcuts.
- **Markdown-ish autoformat:** typing `* ` at line start makes a heading, `- [ ] ` a checkbox,
  `|` starts a table — mapped to correct Org syntax.
- **Properties panel** instead of raw drawers by default (raw view always available).

### Keymaps

- **Default:** platform-standard shortcuts plus a small, documented set for outlining
  (e.g. Alt+↑/↓ move subtree, Tab/Shift+Tab demote/promote or fold, Cmd/Ctrl+Enter new heading).
- **Optional "Emacs" keymap:** `M-RET`, `M-←/→`, `M-↑/↓`, `TAB`/`S-TAB` cycling, `C-c C-t`, etc.
- Built on GPUI's action + keymap system so keymaps are data, not code.

---

## 6. Org fidelity checklist

- Byte-identical round-trip on an unmodified file.
- Respect `#+TODO:`, `#+SEQ_TODO:`, `#+TYP_TODO:`, `#+STARTUP:`, `#+FILETAGS:`, `#+TAGS:`, `#+TITLE:`.
- Generate org-id–compatible `:ID:` properties (UUID) when a heading is linked to.
- Preserve existing tag column alignment and whitespace unless that line is edited.
- Handle LF/CRLF, tabs, BOM, non-UTF-8 (warn), very long lines, and very large files.
- Unknown/unsupported syntax renders as plain text and is never dropped or rewritten.
- Round-trip corpus in CI: org-mode's test files, Worg samples, and real-world notes.

---

## 7. Working without built-in sync

Because users pick their own sync tool, Aster must be robust to external changes:

- Watch the workspace folder; reload clean buffers silently when files change on disk.
- If a buffer has unsaved edits and the file changes externally, show a clear conflict
  prompt (keep mine / take theirs / view diff).
- Recognize and surface common sync conflict files (e.g. `file (conflicted copy).org`,
  `file.sync-conflict-*.org`) rather than indexing them as duplicate notes.
- Atomic saves (write temp file + rename) to avoid corrupting files mid-sync.
- Autosave on by default (newcomer expectation), with debounce.
- Index lives in the app cache dir, never inside the synced folder.

---

## 8. Onboarding

- First launch: choose/create a notes folder → open a **Welcome notebook** (an `.org` file)
  that teaches by doing: headings, TODOs, tags, scheduling, the agenda, capture.
- Starter templates: daily journal, project, meeting notes, reading list.
- Empty states that explain features (e.g. empty agenda explains SCHEDULED/DEADLINE with a button
  to add one).
- Contextual tips the first time a feature is used; dismissible and never repeated.
- "What is this?" tooltips on Org-specific UI (drawers, properties, cookies).

---

## 9. Roadmap

| Phase | Goal | Exit criteria |
|---|---|---|
| **0. Spikes** | Retire the big unknowns | GPUI window builds/runs on macOS and Linux in CI. Minimal multi-line text view with IME + selection. `orgize` round-trips the corpus byte-for-byte (or decision to write our own parser). |
| **1. Core** | `aster-org`, `aster-doc` | Parser, serializer, transactions, undo, structural ops — all tested headless. |
| **2. Editor MVP** | Edit one file well | Open/save/autosave, styled source, folding, selection, clipboard, find/replace, soft wrap. |
| **3. Live preview** | The modern feel | Hidden markup with cursor reveal, checkboxes, link chips, tables, src highlighting, images. |
| **4. Structure & metadata** | Org power, approachable | Heading gutter + drag, move/promote, TODO/priority/tag UI, date picker, properties panel, slash menu, command palette. |
| **5. Workspace** | Many files | Folder workspace, file tree, quick switcher, full-text search, tag browser, `id:`/file links, external-change handling. |
| **6. Organization** | The Org workflow | Agenda views, capture (with global hotkey), refile picker, archive. |
| **7. Onboarding & polish** | Ready for newcomers | Welcome notebook, templates, tips, settings UI, themes (light/dark), Emacs keymap option. |
| **8. Ship 1.0** | Distribution | Signed/notarized builds, installers, auto-update, crash reporting, docs site. |

Phases 2–4 are where most of the effort lands.

### Phase 0 task list

- [x] Set up Cargo workspace (`aster-org`, `aster-editor`, `aster-app`; other crates are added in the phase that needs them)
- [x] CI: fmt, clippy, test, build on macOS and Linux (`.github/workflows/ci.yml`)
- [x] GPUI window with app menu, keybindings, open/save a file (atomic writes)
- [x] Spike: multi-line text element with cursor, selection, IME via `EntityInputHandler`
- [x] Evaluate `gpui-component` editor vs. raw GPUI → raw GPUI ([0002](decisions/0002-editor-foundation.md))
- [x] Assemble round-trip test corpus (`crates/aster-org/tests/corpus`)
- [x] Spike: `orgize` round-trip + incremental reparse; decide parser strategy ([0001](decisions/0001-org-parser.md))
- [x] Record decisions in `docs/decisions/` (lightweight ADRs)
- [x] Linux build verified locally (Nobara 44 / Fedora: fmt, clippy `-D warnings`, tests, build)
- [x] Verify CI green on macOS and Linux (first push)
- [x] Manually test IME (Japanese input) on macOS and Linux (Wayland)

### Phase 1 task list

- [x] `aster-doc`: transactions (batched non-overlapping edits, inverse, position mapping)
- [x] Undo/redo with grouping: typing and delete runs coalesce; newlines, pauses (>1s), cursor
      jumps and commands start new steps; an IME composition is one step
- [x] Structural ops: cycle TODO (honours `#+TODO:`), promote/demote heading or subtree, move
      subtree among siblings, insert sibling heading, toggle checkbox
- [x] Ops preserve CRLF and no-trailing-newline files; randomized test undoes back to exact bytes
- [x] Editor wired to `aster-doc`, with default keybindings for all of the above
- [x] Text storage decision: plain `String` for now ([0003](decisions/0003-text-storage.md))
- [x] Hands-on test of the new commands


### Phase 2 task list

- [x] Soft wrap; headings render larger (variable line heights)
- [x] Virtualized layout: only rows near the viewport are shaped (~3 ms/frame at 16k lines, release)
- [x] Folding: gutter arrows, `…` marker, fold/unfold/fold all/unfold all, `#+STARTUP: overview`;
      cursor and edits never land in hidden text
- [x] Word movement/deletion, double-click word, triple-click line, drag by word/line, cursor blink
- [x] Find bar: smart-case highlight, next/previous, replace, replace all (one undo step)
- [x] Open, Save As, autosave (1 s after typing stops), save on quit
- [x] Edit and Outline menus
- [x] Hands-on test

Deferred: error messages in the UI (currently stderr), reloading files changed on disk (Phase 5),
`#+STARTUP: content`, remembering folds between sessions, rope storage (ADR 0003).


### Phase 3 task list

- [x] Decorations from the syntax tree (`aster-org`) and a tested buffer↔display offset map (`aster-doc`)
- [x] Hide markup unless the cursor is in it: emphasis markers, link brackets/targets, heading
      stars, `#+TITLE:` prefix; checkboxes and bullets render as glyphs
- [x] Styles: bold/italic/underline/strike, code and verbatim chips, links, date pills, large
      title, muted keywords/drawers/planning lines, org-indent style indentation
- [x] Click checkboxes to toggle; Cmd/Ctrl+click links (headings and `id:` jump in-file; URLs,
      `.org` files and other files open via the app)
- [x] Tables: monospace, Tab/Shift+Tab align and move between cells, new row past the end
- [x] Source blocks: tinted, monospace, syntax highlighted (syntect, cached per block)
- [x] Quote blocks with a left bar; comments muted
- [x] Inline images for lines holding only an image link
- [x] Hands-on test

Deferred: hover cursor/tooltip on links, collapsing drawers (properties panel is Phase 4),
`file:x.org::*Heading` search options, LaTeX, footnote rendering, image resizing attributes.


### Phase 4 task list

- [x] `aster-doc::meta`: set TODO state, priority, tags, SCHEDULED/DEADLINE (keeping time and
      repeaters), timestamp dates; move a subtree anywhere with level shifting (all fuzz-tested)
- [x] Clickable TODO/priority/tag pills open menus; clicking a date opens a date picker
- [x] Heading menu (right-click or Cmd/Ctrl+.): state, priority, tags, schedule, deadline, new
      heading, move, indent/outdent, fold
- [x] Date picker with keyboard navigation, Today and Clear
- [x] Slash menu with prefix-ranked filtering
- [x] Command palette with fuzzy search and shortcuts
- [x] Drawers collapse to a "▸ Properties · N" chip (click to edit the raw lines)
- [x] Drag headings by their gutter arrow to reorder or re-nest them
- [ ] Hands-on test on macOS and Linux

Deferred: a structured properties editor (key/value form), tag autocomplete from the whole
workspace (Phase 5 index), keyboard-driven drag, hover handles/"⋯" buttons, multiple selection.

---

## 10. Risks

| Risk | Mitigation |
|---|---|
| GPUI API churn, sparse docs | Pin versions; confine GPUI to UI crates; read Zed source and examples. |
| Text editing complexity (IME, bidi, wrap, perf) | Spike early; keep the first editor small and correct; extensive input tests. |
| Live-preview edge cases (cursor in hidden markup) | Clear reveal rules; property-based tests on cursor movement across hidden ranges. |
| Parser fidelity | Lossless tree; corpus tests in CI; never drop unknown syntax. |
| Accessibility gaps in GPUI | Track upstream; keep semantic structure available; full keyboard operability. |
| Sync-induced corruption/conflicts | Atomic writes, file watching, conflict UI, conflict-file detection. |
| Linux platform variance (compositors, fonts, IME) | Wayland only; test KDE and GNOME; Flatpak for consistent runtime. |
| Scope creep toward "all of Org" | Hold the scope table in §3; new features go to "Future candidates" first. |

---

## 11. Open questions

- ~~Own parser vs. `orgize`~~ — `orgize-ng` with full reparse ([0001](decisions/0001-org-parser.md)).
- ~~Editor from scratch vs. `gpui-component`~~ — raw GPUI ([0002](decisions/0002-editor-foundation.md)).
- Default outlining shortcuts: does Tab fold or demote? (User-test with newcomers.)
- Licensing of bundled fonts/icons; app icon and visual identity.
