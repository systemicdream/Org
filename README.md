# Aster
A Modern Emacs Org Mode Client

Aster is a cross-platform desktop app for Org mode notes, built with [GPUI](https://www.gpui.rs/).
It keeps plain `.org` files as the source of truth and wraps them in a modern, discoverable editor
for people who want Org's workflow without learning Emacs.

See [docs/PLAN.md](docs/PLAN.md) for the roadmap and [docs/decisions](docs/decisions) for design decisions.

## Status

Phase 0 (spikes). Usable for poking at, not for real notes yet.

## Development

Requires Rust (pinned in `rust-toolchain.toml`; rustup installs it automatically).

```sh
cargo run -p aster-app                     # reopens your last folder/file, or a sample
cargo run -p aster-app -- ~/notes          # a folder of .org files becomes the workspace
cargo run -p aster-app -- path/to/notes.org
cargo test --workspace
cargo run --release -p aster-org --example parse_bench
```

Platform notes:

- **macOS:** Xcode command line tools. Shaders compile at launch (`runtime_shaders`), so the
  separate Metal toolchain is not required.
- **Linux:** Wayland only (X11 is not supported). Build deps: `libxkbcommon-dev libxkbcommon-x11-dev
  libwayland-dev libfontconfig-dev libfreetype-dev libvulkan-dev pkg-config` (Debian/Ubuntu names;
  on Fedora: `libxkbcommon-devel libxkbcommon-x11-devel wayland-devel fontconfig-devel
  freetype-devel vulkan-loader-devel`). `libxkbcommon-x11` is only a link-time requirement
  of gpui; the app does not use X11.
- **Windows:** not supported yet (deferred; GPUI supports it, so it can be added later).

## Keys

Standard editing shortcuts, plus (Cmd on macOS, Ctrl on Linux):

| Keys | Action |
|---|---|
| Cmd/Ctrl+Z, Cmd/Ctrl+Shift+Z (Ctrl+Y on Linux) | Undo, redo |
| Cmd/Ctrl+Enter | Toggle checkbox on this line, else cycle the heading's TODO state |
| Cmd/Ctrl+Shift+Enter | Cycle TODO state backwards |
| Alt+Enter | New sibling heading after this subtree |
| Tab, Shift+Tab (on a heading line) | Demote, promote heading with its children |
| Cmd/Ctrl+], Cmd/Ctrl+[ | Demote, promote the current subtree from anywhere in it |
| Alt+Up, Alt+Down | Move subtree among its siblings |
| Click the arrow left of a heading | Fold / unfold it |
| Cmd/Ctrl+Alt+[, Cmd/Ctrl+Alt+] | Fold, unfold the current heading |
| Cmd/Ctrl+K then Cmd/Ctrl+0 / Cmd/Ctrl+J | Fold all, unfold all |
| Alt+←/→ (macOS), Ctrl+←/→ (Linux) | Move by word (add Shift to select; Backspace/Delete to delete) |
| Double-click, triple-click | Select word, line |
| Cmd/Ctrl+F, Cmd+Alt+F (macOS) / Ctrl+H (Linux) | Find, find and replace |
| Cmd/Ctrl+G, Cmd/Ctrl+Shift+G | Next, previous match |
| Cmd/Ctrl+O, Cmd/Ctrl+S, Cmd/Ctrl+Shift+S | Open, save, save as |
| Cmd/Ctrl+Q | Quit |
| Click a checkbox | Tick / untick it |
| Cmd/Ctrl+click a link | Follow it (headings and `id:` in the file; web links, files) |
| Tab, Shift+Tab (in a table) | Align the table and move to the next / previous cell |
| Cmd/Ctrl+Shift+P | Command palette: every command with its shortcut |
| Cmd/Ctrl+Shift+O | Open a notes folder |
| Cmd/Ctrl+P | Go to file (fuzzy, by name or `#+TITLE`) |
| Cmd/Ctrl+Shift+F | Search all files |
| Cmd/Ctrl+B | Show / hide the sidebar (file tree and tags) |
| Click a TODO keyword, priority, tag or date | Change it from a menu / date picker |
| Right-click a heading, or Cmd/Ctrl+. | Heading menu (state, priority, tags, schedule, deadline, move…) |
| `/` at the start of a line or after a space | Slash menu: headings, to-dos, lists, tables, code, quotes, dates… |
| Drag the arrow left of a heading | Move it (and its children); drag sideways to change its level |
| Click a "▸ Properties" chip | Show / hide a property drawer |

Files save automatically one second after you stop typing (atomic write), and on quit.
A file with `#+STARTUP: overview` opens with every heading folded.

## Notes folder

Open a folder (or a file inside it) and Aster shows its `.org` files in a sidebar with your tags.
`id:` links and `file:other.org::*Heading` links work across files. Aster watches the folder:
changes made elsewhere (Emacs, a sync tool) show up immediately, and if a file you have unsaved
edits in changes on disk, Aster asks which version to keep. Sync-conflict copies are marked ⚠.
Aster stores nothing in your notes folder; it remembers the last folder and file in its config
directory (override with `ASTER_SESSION_FILE`).

## Live preview

Aster shows Org markup formatted — `*bold*` as **bold**, `[[url][text]]` as a link, `- [ ]` as a
checkbox — and reveals the raw text only where the cursor is, so the file stays plain Org.
Code blocks are syntax highlighted, tables are set in a monospace font, and a line holding only
an image link (`[[file:photo.png]]`) shows the image.
