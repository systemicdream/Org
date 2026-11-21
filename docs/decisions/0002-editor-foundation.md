# 0002 — Editor foundation: custom element on raw GPUI

- Status: accepted (Phase 0)
- Date: 2026-11-21

## Context

The editor is the product. Options: build on GPUI primitives (`Element`, `ShapedLine`,
`EntityInputHandler`), adopt `gpui-component`'s multi-line input/code editor, or reuse Zed's
`editor` crate (not viable: tightly coupled to Zed's workspace, language, and project crates).

## Findings

The spike in `crates/aster-editor` (~1,000 lines) implements, on raw GPUI 0.2.2:
multi-line layout, cursor and keyboard/mouse selection, vertical movement with goal column,
clipboard, CRLF-preserving newlines, IME (marked text, character palette, candidate-window
bounds), auto-scroll to cursor, and styling driven by the Org syntax tree.

Live preview (hiding markup, inline widgets, different font sizes per heading) needs full
control of layout and hit-testing, which a generic code-editor component would fight against.

## Decision

Build Aster's editor directly on GPUI primitives. Use `gpui-component` selectively for
non-editor UI (lists, popovers, inputs in dialogs) if it earns its place — evaluate in Phase 4/5.

## Known gaps to close in Phase 2

- Rope buffer + undo/redo transactions (`aster-doc`)
- Soft wrap (`shape_text` / `WrappedLine`) and virtualized layout (only shape visible lines)
- Word movement, double/triple-click selection, cursor blink
- Variable line heights (needed for heading sizes and live preview)
