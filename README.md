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
cargo run -p aster-app                     # opens a built-in sample
cargo run -p aster-app -- path/to/notes.org
cargo test --workspace
cargo run --release -p aster-org --example parse_bench
```

Platform notes:

- **macOS:** Xcode command line tools. Shaders compile at launch (`runtime_shaders`), so the
  separate Metal toolchain is not required.
- **Linux:** `libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-xcb-dev libxcb1-dev
  libfontconfig-dev libfreetype-dev libvulkan-dev pkg-config` (Debian/Ubuntu names).
- **Windows:** not supported yet (deferred; GPUI supports it, so it can be added later).

Keys: platform-standard editing shortcuts; Cmd/Ctrl+S saves (atomic write), Cmd/Ctrl+Q quits.
