# Aster
A Modern Emacs Org Mode Client

Aster is a cross-platform desktop app for Org mode notes, built with [GPUI](https://www.gpui.rs/).
It keeps plain `.org` files as the source of truth and wraps them in a modern, discoverable editor
for people who want Org's workflow without learning Emacs.

See [docs/PLAN.md](docs/PLAN.md) for the roadmap and [docs/decisions](docs/decisions) for design decisions.

## Status

**0.1.0, early development.** Aster works for everyday notes, but expect rough edges, and keep
a backup of notes you care about. Versions stay at 0.x until it has seen real use; see
[CHANGELOG.md](CHANGELOG.md) for what's in each release.

## Development

Requires Rust (pinned in `rust-toolchain.toml`; rustup installs it automatically).

```sh
cargo run -p aster-app                     # reopens your last folder/file, or the welcome screen
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

## Installing

Download the package for your system from the
[latest release](https://github.com/systemicdream/Aster/releases/latest):

| System | Package | Install |
|---|---|---|
| macOS 11+ (Apple silicon, Intel) | `Aster-<version>-macos-universal.dmg` | Drag Aster to Applications (first launch: see below) |
| Linux (any distribution) | `Aster-<version>-x86_64.flatpak` | `flatpak install --user Aster-<version>-x86_64.flatpak` |
| Debian 12+, Ubuntu 22.04+ | `aster_<version>_amd64.deb` | `sudo apt install ./aster_<version>_amd64.deb` |
| Fedora | `aster-<version>-1.x86_64.rpm` | `sudo dnf install ./aster-<version>-1.x86_64.rpm` |
| Other Linux | `aster-<version>-linux-x86_64.tar.gz` | Extract, then `./install.sh` |

**macOS, first launch:** Aster isn't notarized by Apple yet, so macOS stops it the first time.
Open it once, then go to **System Settings → Privacy & Security** and click **Open Anyway**.

**Linux:** Aster needs a Wayland session (X11 isn't supported). The Flatpak is the
recommended package; it isn't on Flathub yet, so it doesn't update by itself.

Aster checks GitHub once a day for a new version and tells you when there is one (Settings →
About can turn this off; nothing about you or your notes is sent). If Aster crashes, it keeps a
report on your computer and offers to open a GitHub issue with it the next time; nothing is
sent unless you choose to.

### From source

```sh
scripts/bundle-macos.sh     # macOS: builds target/release/Aster.app (drag it to /Applications)
scripts/install-linux.sh    # Linux: installs to ~/.local/bin with a launcher entry and icon
```

On Linux the window's Wayland app id (`io.github.systemicdream.Aster`) matches the desktop entry,
which is how the compositor finds the icon. The icon artwork lives in
`assets/branding/aster-icon.png`; `scripts/make-icons.py` regenerates every size from it.
Building release packages is described in [docs/RELEASING.md](docs/RELEASING.md).

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
| Cmd/Ctrl+Shift+A, Cmd/Ctrl+Shift+T | Agenda, to-do list |
| Cmd/Ctrl+N | Capture a task, note or journal entry |
| Cmd/Ctrl+Shift+R | Refile the heading at the cursor |
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

## Agenda, capture, refile

The agenda (Cmd/Ctrl+Shift+A) gathers what's scheduled, due and dated across the folder: a day
or week view (repeating dates expanded; overdue tasks and upcoming deadlines on today), the list
of open to-dos, and Org tag/property searches like `+work-home TODO="NEXT"`. In the agenda:
`j`/`k` move, Enter opens, `x` marks done, `d`/`w`/`t`/`s` switch views, `b`/`f` step, `.` jumps
to today.

Capture (Cmd/Ctrl+N) adds a task or note to `inbox.org`, or a timestamped entry under today in
`journal.org`. Refile moves a heading under any heading (or to the top level of any file) in the
folder. Archive moves it to `<file>_archive` with Org's archive properties.

## Live preview

Aster shows Org markup formatted — `*bold*` as **bold**, `[[url][text]]` as a link, `- [ ]` as a
checkbox — and reveals the raw text only where the cursor is, so the file stays plain Org.
Code blocks are syntax highlighted, tables are set in a monospace font, and a line holding only
an image link (`[[file:photo.png]]`) shows the image.
