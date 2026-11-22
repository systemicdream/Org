//! The Org editor view, built on raw GPUI primitives.
//!
//! - IME / dead keys / emoji palette through [`EntityInputHandler`](gpui::EntityInputHandler)
//! - soft-wrapped, virtualized layout with larger headings ([`element`])
//! - folding, word movement, multi-click selection, blinking cursor
//! - every change goes through an [`aster_doc::Document`] transaction, so it is undoable
//! - outline commands (TODO cycling, promote/demote, move subtree, new heading, checkboxes)
//! - search highlighting and replace, driven by [`FindBar`]
//! - a single-line mode, used for the find bar's inputs

mod element;
mod find_bar;
mod highlight;
mod input;
mod popup;
mod preview;
mod spans;

use std::collections::{BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use aster_doc::display::RowDisplay;
use aster_doc::folds::Folds;
use aster_doc::{Document, EditOrigin, Selection, ops, search, text};
use aster_org::{DecoKind, Decoration};
use aster_ui::ActiveTheme;
use element::EditorElement;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, EventEmitter, FocusHandle, Focusable, Font,
    Hsla, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ScrollHandle, SharedString, Subscription, Task, TextRun, Window, WrappedLine, actions, div,
    point, prelude::*, px,
};
use popup::{NavKey, Popup};
use preview::RowKind;

pub use find_bar::{FindBar, FindBarEvent};

actions!(
    editor,
    [
        Backspace,
        Delete,
        DeleteWordLeft,
        DeleteWordRight,
        Left,
        Right,
        Up,
        Down,
        WordLeft,
        WordRight,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectWordLeft,
        SelectWordRight,
        SelectAll,
        LineStart,
        LineEnd,
        SelectLineStart,
        SelectLineEnd,
        DocStart,
        DocEnd,
        Newline,
        ShiftEnter,
        Tab,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
        Undo,
        Redo,
        Cancel,
        // Outline commands
        ToggleDone,
        CycleTodoBackward,
        InsertHeading,
        PromoteSubtree,
        DemoteSubtree,
        PromoteHeading,
        DemoteHeading,
        MoveSubtreeUp,
        MoveSubtreeDown,
        Outdent,
        OpenHeadingMenu,
        /// Move the heading at the cursor under another heading or file (handled by the app).
        Refile,
        /// Move the heading at the cursor to the file's archive (handled by the app).
        ArchiveSubtree,
        // Folding
        ToggleFold,
        Fold,
        Unfold,
        FoldAll,
        UnfoldAll,
    ]
);

const KEY_CONTEXT: &str = "OrgEditor";
const BLINK_INTERVAL: Duration = Duration::from_millis(530);

/// Register the editor's default (platform-standard) keybindings.
pub fn bind_keys(cx: &mut App) {
    let ctx = Some(KEY_CONTEXT);
    let mut bindings = vec![
        KeyBinding::new("backspace", Backspace, ctx),
        KeyBinding::new("delete", Delete, ctx),
        KeyBinding::new("left", Left, ctx),
        KeyBinding::new("right", Right, ctx),
        KeyBinding::new("up", Up, ctx),
        KeyBinding::new("down", Down, ctx),
        KeyBinding::new("shift-left", SelectLeft, ctx),
        KeyBinding::new("shift-right", SelectRight, ctx),
        KeyBinding::new("shift-up", SelectUp, ctx),
        KeyBinding::new("shift-down", SelectDown, ctx),
        KeyBinding::new("home", LineStart, ctx),
        KeyBinding::new("end", LineEnd, ctx),
        KeyBinding::new("shift-home", SelectLineStart, ctx),
        KeyBinding::new("shift-end", SelectLineEnd, ctx),
        KeyBinding::new("enter", Newline, ctx),
        KeyBinding::new("shift-enter", ShiftEnter, ctx),
        KeyBinding::new("tab", Tab, ctx),
        KeyBinding::new("escape", Cancel, ctx),
        KeyBinding::new("secondary-a", SelectAll, ctx),
        KeyBinding::new("secondary-v", Paste, ctx),
        KeyBinding::new("secondary-c", Copy, ctx),
        KeyBinding::new("secondary-x", Cut, ctx),
        KeyBinding::new("secondary-z", Undo, ctx),
        KeyBinding::new("secondary-shift-z", Redo, ctx),
        // Outline commands. Defaults are chosen to be familiar from other apps; Org/Emacs-style
        // bindings will live in an optional keymap.
        KeyBinding::new("shift-tab", Outdent, ctx),
        KeyBinding::new("secondary-enter", ToggleDone, ctx),
        KeyBinding::new("secondary-shift-enter", CycleTodoBackward, ctx),
        KeyBinding::new("alt-enter", InsertHeading, ctx),
        KeyBinding::new("secondary-]", DemoteSubtree, ctx),
        KeyBinding::new("secondary-[", PromoteSubtree, ctx),
        KeyBinding::new("alt-up", MoveSubtreeUp, ctx),
        KeyBinding::new("alt-down", MoveSubtreeDown, ctx),
        KeyBinding::new("secondary-.", OpenHeadingMenu, ctx),
        // Folding (VS Code-style).
        KeyBinding::new("secondary-alt-[", Fold, ctx),
        KeyBinding::new("secondary-alt-]", Unfold, ctx),
        KeyBinding::new("secondary-k secondary-0", FoldAll, ctx),
        KeyBinding::new("secondary-k secondary-j", UnfoldAll, ctx),
    ];
    if cfg!(target_os = "macos") {
        bindings.extend([
            KeyBinding::new("cmd-left", LineStart, ctx),
            KeyBinding::new("cmd-right", LineEnd, ctx),
            KeyBinding::new("cmd-shift-left", SelectLineStart, ctx),
            KeyBinding::new("cmd-shift-right", SelectLineEnd, ctx),
            KeyBinding::new("cmd-up", DocStart, ctx),
            KeyBinding::new("cmd-down", DocEnd, ctx),
            KeyBinding::new("alt-left", WordLeft, ctx),
            KeyBinding::new("alt-right", WordRight, ctx),
            KeyBinding::new("alt-shift-left", SelectWordLeft, ctx),
            KeyBinding::new("alt-shift-right", SelectWordRight, ctx),
            KeyBinding::new("alt-backspace", DeleteWordLeft, ctx),
            KeyBinding::new("alt-delete", DeleteWordRight, ctx),
            KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, ctx),
        ]);
    } else {
        bindings.extend([
            KeyBinding::new("ctrl-y", Redo, ctx),
            KeyBinding::new("ctrl-home", DocStart, ctx),
            KeyBinding::new("ctrl-end", DocEnd, ctx),
            KeyBinding::new("ctrl-left", WordLeft, ctx),
            KeyBinding::new("ctrl-right", WordRight, ctx),
            KeyBinding::new("ctrl-shift-left", SelectWordLeft, ctx),
            KeyBinding::new("ctrl-shift-right", SelectWordRight, ctx),
            KeyBinding::new("ctrl-backspace", DeleteWordLeft, ctx),
            KeyBinding::new("ctrl-delete", DeleteWordRight, ctx),
        ]);
    }
    cx.bind_keys(bindings);
}

/// Editor commands for the command palette: (name, action).
pub fn commands() -> Vec<(&'static str, Box<dyn gpui::Action>)> {
    vec![
        ("Toggle done (checkbox or TODO state)", Box::new(ToggleDone)),
        ("Cycle TODO state backwards", Box::new(CycleTodoBackward)),
        ("New heading below", Box::new(InsertHeading)),
        ("Heading menu…", Box::new(OpenHeadingMenu)),
        ("Refile heading…", Box::new(Refile)),
        ("Archive heading", Box::new(ArchiveSubtree)),
        ("Move heading up", Box::new(MoveSubtreeUp)),
        ("Move heading down", Box::new(MoveSubtreeDown)),
        ("Indent heading (with children)", Box::new(DemoteSubtree)),
        ("Outdent heading (with children)", Box::new(PromoteSubtree)),
        ("Indent heading only", Box::new(DemoteHeading)),
        ("Outdent heading only", Box::new(PromoteHeading)),
        ("Fold heading", Box::new(Fold)),
        ("Unfold heading", Box::new(Unfold)),
        ("Toggle fold", Box::new(ToggleFold)),
        ("Fold all", Box::new(FoldAll)),
        ("Unfold all", Box::new(UnfoldAll)),
        ("Undo", Box::new(Undo)),
        ("Redo", Box::new(Redo)),
        ("Select all", Box::new(SelectAll)),
        ("Cut", Box::new(Cut)),
        ("Copy", Box::new(Copy)),
        ("Paste", Box::new(Paste)),
    ]
}

/// Human-readable shortcut for a binding, e.g. "⌘⇧P" on macOS or "Ctrl+Shift+P" elsewhere.
pub fn format_keystrokes(binding: &KeyBinding) -> String {
    let mac = cfg!(target_os = "macos");
    binding
        .keystrokes()
        .iter()
        .map(|k| {
            let k = k.inner();
            let m = &k.modifiers;
            let mut out = String::new();
            let mods: [(bool, &str, &str); 4] = [
                (m.control, "⌃", "Ctrl+"),
                (m.alt, "⌥", "Alt+"),
                (m.shift, "⇧", "Shift+"),
                (m.platform, "⌘", "Super+"),
            ];
            for (on, mac_sym, other) in mods {
                if on {
                    out.push_str(if mac { mac_sym } else { other });
                }
            }
            let key = match k.key.as_str() {
                "enter" => "↵".to_string(),
                "up" => "↑".to_string(),
                "down" => "↓".to_string(),
                "left" => "←".to_string(),
                "right" => "→".to_string(),
                "escape" => "Esc".to_string(),
                "tab" => "Tab".to_string(),
                "backspace" => "⌫".to_string(),
                key if key.chars().count() == 1 => key.to_uppercase(),
                key => {
                    let mut c = key.chars();
                    c.next()
                        .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
                }
            };
            out.push_str(&key);
            out
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEvent {
    /// The text changed.
    Edited,
    /// Enter (or Shift+Enter when `back`) in a single-line editor.
    Confirm { back: bool },
    /// Escape.
    Cancel,
    /// Up/Down in a single-line editor (e.g. to move through a list below it).
    Navigate { up: bool },
    /// Cmd/Ctrl+click on a link the editor can't follow itself (URLs, files).
    OpenLink(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    /// A full Org document: wrapping, folding, outline commands.
    Full,
    /// A one-line plain input (e.g. the find bar). Enter emits [`EditorEvent::Confirm`].
    SingleLine,
}

/// Pressing in a heading's gutter starts a potential drag; it becomes a move once the mouse
/// travels a few pixels, otherwise the press is a click that toggles the fold.
#[derive(Debug, Clone)]
struct HeadingDrag {
    heading: usize,
    origin: Point<Pixels>,
    active: bool,
    /// Where it would land, and the indicator line (window y, left x).
    target: Option<(aster_doc::meta::DropTarget, Pixels, Pixels)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Granularity {
    Char,
    Word,
    Line,
}

/// Text metrics captured during paint, used when actions need to lay out lines.
#[derive(Clone)]
struct Metrics {
    font: Font,
    mono_family: SharedString,
    font_size: Pixels,
    color: Hsla,
    wrap_width: Option<Pixels>,
}

/// Horizontal indent per outline level (org-indent style).
fn indent_step() -> Pixels {
    px(22.)
}

/// Per-line presentation, recomputed on every change.
struct RowMeta {
    kind: RowKind,
    /// Outline depth used for indentation.
    depth: usize,
    /// Hash of the line's text, for the wrap cache.
    hash: u64,
}

/// Where each visible row was laid out in the last frame.
struct Layout {
    bounds: Bounds<Pixels>,
    text_left: Pixels,
    rows: Vec<RowLayout>,
}

struct RowLayout {
    row: usize,
    /// Relative to `Layout::bounds.top()`.
    top: Pixels,
    /// Horizontal offset from `Layout::text_left`.
    indent: Pixels,
    /// What is shown for this line (markup hidden unless the cursor is in it).
    display: RowDisplay,
    /// Space below the text for an inline image.
    extra: Pixels,
    line_height: Pixels,
    /// Shaped only for rows near the viewport; others are shaped on demand.
    wrapped: Option<WrappedLine>,
    visual_lines: usize,
}

impl RowLayout {
    fn height(&self) -> Pixels {
        self.line_height * self.visual_lines as f32 + self.extra
    }

    /// Height of the text part (without an inline image).
    fn text_height(&self) -> Pixels {
        self.line_height * self.visual_lines as f32
    }
}

struct SearchState {
    query: String,
    matches: Vec<Range<usize>>,
    active: Option<usize>,
}

pub struct OrgEditor {
    /// Theme colors (docs/DESIGN.md), updated when the theme changes.
    colors: aster_ui::Colors,
    dark: bool,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    mode: EditorMode,
    placeholder: Option<SharedString>,
    doc: Document,
    /// Byte offset of the start of each line. Always starts with 0.
    line_starts: Vec<usize>,
    /// Org syntax styling, sorted by start. Recomputed on every change.
    org_spans: spans::Spans,
    /// Highlighted source blocks, keyed by hash of (language, code), so unchanged blocks are
    /// not re-highlighted on every keystroke.
    highlight_cache: HashMap<u64, std::rc::Rc<spans::Spans>>,
    /// Folder of the open file, for resolving relative links and images.
    base_dir: Option<PathBuf>,
    /// Rows that show an inline image (a line holding only an image link).
    row_images: HashMap<usize, PathBuf>,
    /// Natural size of loaded images.
    image_sizes: HashMap<PathBuf, gpui::Size<Pixels>>,
    row_meta: Vec<RowMeta>,
    /// Monospace font family, resolved from installed fonts on first paint.
    mono_family: Option<SharedString>,
    folds: Folds,
    /// Drawers (`:PROPERTIES:` …) shown expanded, by start offset. Drawers are collapsed into a
    /// chip by default.
    open_drawers: BTreeSet<usize>,
    drawer_cache: Vec<(Range<usize>, Range<usize>)>,
    /// Document version the folds were last remapped for.
    synced_version: u64,
    hidden: Vec<Range<usize>>,
    visible_rows: Vec<usize>,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    /// Preferred x position for vertical movement.
    goal_x: Option<Pixels>,
    is_selecting: bool,
    /// A heading being dragged from its gutter (see [`HeadingDrag`]).
    heading_drag: Option<HeadingDrag>,
    drag: (Granularity, Range<usize>),
    autoscroll: bool,
    metrics: Option<Metrics>,
    layout: Option<Layout>,
    /// Visual line count per (line text, scale, wrap width), so off-screen rows keep stable
    /// heights without being shaped.
    wrap_cache: HashMap<u64, usize>,
    search: Option<SearchState>,
    cursor_visible: bool,
    blink_task: Option<Task<()>>,
    /// Tags used across the notes folder, most used first (for suggestions).
    known_tags: Vec<String>,
    /// Open menu, date picker, tag input or slash menu.
    popup: Option<Popup>,
    /// Where the last popup was shown, so follow-up popups appear in the same place.
    last_popup_anchor: Point<Pixels>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for OrgEditor {}

impl OrgEditor {
    pub fn new(text: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_mode(text, EditorMode::Full, window, cx)
    }

    pub fn single_line(
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::with_mode(String::new(), EditorMode::SingleLine, window, cx);
        this.placeholder = Some(placeholder.into());
        this
    }

    fn with_mode(
        text: String,
        mode: EditorMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let subscriptions = vec![
            cx.on_focus(&focus_handle, window, |this, _, cx| this.restart_blink(cx)),
            cx.on_blur(&focus_handle, window, |this, _, cx| {
                this.blink_task = None;
                this.cursor_visible = true;
                cx.notify();
            }),
            // Re-style when the theme (light/dark) changes.
            cx.observe_global::<aster_ui::Theme>(|this, cx| {
                this.colors = cx.theme().colors;
                this.dark = cx.theme().is_dark();
                this.highlight_cache.clear();
                this.recompute_lines();
                cx.notify();
            }),
        ];
        let mut this = Self {
            colors: cx.theme().colors,
            dark: cx.theme().is_dark(),
            focus_handle,
            scroll_handle: ScrollHandle::new(),
            mode,
            placeholder: None,
            doc: Document::new(text),
            line_starts: Vec::new(),
            row_meta: Vec::new(),
            mono_family: None,
            org_spans: Vec::new(),
            highlight_cache: HashMap::new(),
            base_dir: None,
            row_images: HashMap::new(),
            image_sizes: HashMap::new(),
            folds: Folds::default(),
            open_drawers: BTreeSet::new(),
            drawer_cache: Vec::new(),
            synced_version: 0,
            hidden: Vec::new(),
            visible_rows: Vec::new(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            goal_x: None,
            is_selecting: false,
            heading_drag: None,
            drag: (Granularity::Char, 0..0),
            autoscroll: false,
            metrics: None,
            layout: None,
            wrap_cache: HashMap::new(),
            search: None,
            cursor_visible: true,
            blink_task: None,
            known_tags: Vec::new(),
            popup: None,
            last_popup_anchor: Point::default(),
            _subscriptions: subscriptions,
        };
        if mode == EditorMode::Full && startup_folded(this.doc.text()) {
            this.folds.fold_all(this.doc.headings(), this.doc.text());
        }
        this.recompute_lines();
        this
    }

    pub fn text(&self) -> &str {
        self.doc.text()
    }

    /// Tags to suggest when tagging a heading (most used first).
    pub fn set_known_tags(&mut self, tags: Vec<String>) {
        self.known_tags = tags;
    }

    /// Set the folder of the open file (relative image links resolve against it).
    pub fn set_base_dir(&mut self, dir: Option<PathBuf>, cx: &mut Context<Self>) {
        self.base_dir = dir;
        self.recompute_lines();
        cx.notify();
    }

    /// Resolve a link target to a local file path (`file:`, `~/`, relative paths).
    fn resolve_path(&self, target: &str) -> Option<PathBuf> {
        if target.contains("://") {
            return None;
        }
        let path = target.strip_prefix("file:").unwrap_or(target);
        let path = path.split("::").next().unwrap_or(path);
        let path = match path.strip_prefix("~/") {
            Some(rest) => std::env::home_dir()?.join(rest),
            None => PathBuf::from(path),
        };
        Some(if path.is_relative() {
            self.base_dir
                .as_deref()
                .unwrap_or(Path::new("."))
                .join(path)
        } else {
            path
        })
    }

    /// Size an image to fit `max_width` (and a maximum height).
    fn image_display_size(&self, row: usize, max_width: Pixels) -> Option<gpui::Size<Pixels>> {
        let natural = self.image_sizes.get(self.row_images.get(&row)?)?;
        if natural.width <= px(0.) || natural.height <= px(0.) {
            return None;
        }
        let max_height = px(420.);
        let scale = (max_width / natural.width)
            .min(max_height / natural.height)
            .min(1.0);
        Some(gpui::size(natural.width * scale, natural.height * scale))
    }

    /// Replace the whole text (e.g. a single-line input being reset). Not undoable.
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.doc = Document::new(text);
        self.folds = Folds::default();
        self.set_selection(Selection::cursor(text.len()));
        self.after_change(cx);
    }

    /// The selected text (empty when nothing is selected).
    /// The document being edited (read-only).
    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Start offset of the heading whose section contains the cursor.
    pub fn heading_start_at_cursor(&self) -> Option<usize> {
        self.heading_at_cursor().map(|h| h.range.start)
    }

    /// Apply a document operation computed elsewhere (e.g. refile) as one undo step.
    pub fn apply_result(&mut self, result: ops::OpResult, cx: &mut Context<Self>) {
        self.apply_op(Some(result), cx);
    }

    /// Move the cursor to `offset` (opening folds/drawers around it) and scroll it into view.
    pub fn jump_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.select_range(offset..offset, cx);
    }

    /// Select `range` (e.g. a search match), opening folds/drawers around it.
    pub fn select_range(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let len = self.doc.text().len();
        let range = range.start.min(len)..range.end.min(len);
        let folded = self
            .folds
            .reveal(range.start, self.doc.headings(), self.doc.text());
        if folded | self.reveal_drawer(range.start) {
            self.recompute_lines();
        }
        self.doc.seal_undo_group();
        self.set_selection(Selection {
            anchor: range.start,
            head: range.end,
        });
        self.autoscroll = true;
        self.restart_blink(cx);
        cx.notify();
    }

    /// Replace the text with `new_text` (e.g. the file changed on disk) as one undoable edit
    /// covering only the part that differs, so the cursor and folds elsewhere stay put.
    pub fn reload_text(&mut self, new_text: &str, cx: &mut Context<Self>) {
        let old = self.doc.text();
        if old == new_text {
            return;
        }
        let prefix = old
            .bytes()
            .zip(new_text.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        let max_suffix = old.len().min(new_text.len()) - prefix;
        let suffix = old
            .bytes()
            .rev()
            .zip(new_text.bytes().rev())
            .take(max_suffix)
            .take_while(|(a, b)| a == b)
            .count();
        // Keep the edit on character boundaries.
        let mut start = prefix;
        while !old.is_char_boundary(start) || !new_text.is_char_boundary(start) {
            start -= 1;
        }
        let mut old_end = old.len() - suffix;
        let mut new_end = new_text.len() - suffix;
        while !old.is_char_boundary(old_end) || !new_text.is_char_boundary(new_end) {
            old_end += 1;
            new_end += 1;
        }
        let tx = aster_doc::Transaction::replace(start..old_end, &new_text[start..new_end]);
        let before = self.selection();
        let after = before.map(&tx);
        self.doc.seal_undo_group();
        self.doc.apply(tx, before, after, EditOrigin::Other);
        self.doc.seal_undo_group();
        self.set_selection(after);
        self.after_change(cx);
    }

    pub fn selected_text(&self) -> &str {
        &self.doc.text()[self.selected_range.clone()]
    }

    /// Select everything (used when focusing an input).
    pub fn select_all_text(&mut self, cx: &mut Context<Self>) {
        self.set_selection(Selection {
            anchor: 0,
            head: self.doc.text().len(),
        });
        cx.notify();
    }

    // ---- buffer -------------------------------------------------------------------------

    /// Replace `range` with `new_text` as one (possibly coalesced) undo step and put the
    /// cursor after the inserted text.
    fn edit(
        &mut self,
        range: Range<usize>,
        new_text: &str,
        origin: EditOrigin,
        cx: &mut Context<Self>,
    ) {
        // Edits touching folded text unfold it first, so nothing changes out of sight.
        let mut revealed = false;
        for pos in [range.start, range.end] {
            revealed |= self.folds.reveal(pos, self.doc.headings(), self.doc.text());
            revealed |= self.reveal_drawer(pos);
        }
        if revealed {
            self.recompute_lines();
        }
        let new_text = if self.mode == EditorMode::SingleLine {
            new_text.replace(['\r', '\n'], " ")
        } else {
            new_text.to_string()
        };
        let before = self.selection();
        let cursor = range.start + new_text.len();
        self.doc.apply(
            aster_doc::Transaction::replace(range, new_text),
            before,
            Selection::cursor(cursor),
            origin,
        );
        self.set_selection(Selection::cursor(cursor));
        self.after_change(cx);
    }

    fn after_change(&mut self, cx: &mut Context<Self>) {
        // Follow the text change with view state, once per document version.
        if self.synced_version != self.doc.version() {
            if let Some(tx) = self.doc.last_change() {
                self.folds.remap(tx, self.doc.headings(), self.doc.text());
                let starts: BTreeSet<usize> = self
                    .compute_drawers()
                    .into_iter()
                    .map(|(r, _)| r.start)
                    .collect();
                self.open_drawers = self
                    .open_drawers
                    .iter()
                    .map(|&s| tx.map(s, true))
                    .filter(|s| starts.contains(s))
                    .collect();
            }
            self.synced_version = self.doc.version();
        }
        self.recompute_lines();
        self.refresh_search();
        self.popup_after_change();
        self.marked_range = None;
        self.goal_x = None;
        self.autoscroll = true;
        self.restart_blink(cx);
        cx.emit(EditorEvent::Edited);
        cx.notify();
    }

    /// Apply a structural command's result, if it produced one.
    fn apply_op(&mut self, result: Option<ops::OpResult>, cx: &mut Context<Self>) {
        if self.mode != EditorMode::Full {
            return;
        }
        let Some(result) = result else { return };
        if result.tx.is_empty() {
            // Nothing to change; just move (e.g. Tab to the next table cell).
            self.move_to(result.selection.head, cx);
            return;
        }
        let before = self.selection();
        self.doc
            .apply(result.tx, before, result.selection, EditOrigin::Other);
        self.doc.seal_undo_group();
        self.set_selection(result.selection);
        self.after_change(cx);
    }

    fn recompute_lines(&mut self) {
        let text = self.doc.text();
        self.line_starts.clear();
        self.line_starts.push(0);
        self.line_starts
            .extend(text.match_indices('\n').map(|(i, _)| i + 1));
        let kinds = if self.mode == EditorMode::Full {
            preview::row_kinds(
                &self.line_starts,
                text.len(),
                self.doc.headings(),
                self.doc.decorations(),
            )
        } else {
            vec![(RowKind::Body, 0); self.line_starts.len()]
        };
        self.row_meta = kinds
            .into_iter()
            .enumerate()
            .map(|(row, (kind, depth))| {
                let mut h = DefaultHasher::new();
                self.doc.text()[self.line_range(row)].hash(&mut h);
                RowMeta {
                    kind,
                    depth,
                    hash: h.finish(),
                }
            })
            .collect();
        self.org_spans = if self.mode == EditorMode::Full {
            let mut s = spans::style_spans(self.doc.headings(), self.doc.text(), &self.colors);
            s.extend(self.code_spans());
            // Stable: spans starting at the same offset keep their override order.
            s.sort_by_key(|(r, _)| r.start);
            s
        } else {
            Vec::new()
        };
        self.row_images = if self.mode == EditorMode::Full {
            self.find_image_rows()
        } else {
            HashMap::new()
        };

        self.drawer_cache = if self.mode == EditorMode::Full {
            self.compute_drawers()
        } else {
            Vec::new()
        };
        let text = self.doc.text();
        let mut hidden = self.folds.hidden_ranges(self.doc.headings(), text);
        if self.mode == EditorMode::Full {
            hidden.extend(
                self.drawers()
                    .iter()
                    .filter(|(r, _)| !self.open_drawers.contains(&r.start))
                    .map(|(_, body)| body.clone())
                    .filter(|b| !b.is_empty()),
            );
        }
        hidden.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for r in hidden {
            match merged.last_mut() {
                Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                _ => merged.push(r),
            }
        }
        self.hidden = merged;
        let hidden = &self.hidden;
        self.visible_rows = (0..self.line_starts.len())
            .filter(|&row| {
                let start = self.line_starts[row];
                let i = hidden.partition_point(|r| r.end <= start);
                hidden.get(i).is_none_or(|r| start < r.start)
            })
            .collect();
    }

    /// Lines that consist only of a link to an image file (Org's inline images).
    fn find_image_rows(&self) -> HashMap<usize, PathBuf> {
        const EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"];
        let text = self.doc.text();
        let mut out = HashMap::new();
        for d in self.doc.decorations() {
            let DecoKind::Link {
                target,
                has_description: false,
            } = &d.kind
            else {
                continue;
            };
            let row = self.row_for_offset(d.range.start);
            if text[self.line_range(row)].trim() != &text[d.range.clone()] {
                continue;
            }
            let Some(path) = self.resolve_path(target) else {
                continue;
            };
            let is_image = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| EXTS.contains(&e.to_ascii_lowercase().as_str()));
            if is_image {
                out.insert(row, path);
            }
        }
        out
    }

    /// Syntax-highlight spans for every source block (absolute offsets), using the cache.
    fn code_spans(&mut self) -> spans::Spans {
        let mut out = spans::Spans::new();
        let mut used = HashMap::new();
        for d in self.doc.decorations() {
            let DecoKind::BlockContent {
                lang: Some(lang), ..
            } = &d.kind
            else {
                continue;
            };
            let code = &self.doc.text()[d.range.clone()];
            let mut h = DefaultHasher::new();
            (lang, code, self.dark).hash(&mut h);
            let key = h.finish();
            let spans =
                self.highlight_cache.get(&key).cloned().unwrap_or_else(|| {
                    std::rc::Rc::new(highlight::highlight(lang, code, self.dark))
                });
            out.extend(
                spans
                    .iter()
                    .map(|(r, st)| (r.start + d.range.start..r.end + d.range.start, *st)),
            );
            used.insert(key, spans);
        }
        // Keep only blocks that still exist.
        self.highlight_cache = used;
        out
    }

    /// Byte range of line `row`, excluding its line terminator (`\n` or `\r\n`).
    fn line_range(&self, row: usize) -> Range<usize> {
        let text = self.doc.text();
        let start = self.line_starts[row];
        let mut end = self
            .line_starts
            .get(row + 1)
            .map_or(text.len(), |next| next - 1);
        if end > start && text.as_bytes()[end - 1] == b'\r' && end < text.len() {
            end -= 1;
        }
        start..end
    }

    fn row_for_offset(&self, offset: usize) -> usize {
        self.line_starts.partition_point(|&s| s <= offset) - 1
    }

    /// The hidden range containing `offset`, if any.
    fn hidden_at(&self, offset: usize) -> Option<Range<usize>> {
        let i = self.hidden.partition_point(|r| r.end <= offset);
        self.hidden.get(i).filter(|r| r.start <= offset).cloned()
    }

    /// Move `offset` out of folded text: forward to after it, or back to the heading line end.
    fn clamp_visible(&self, offset: usize, forward: bool) -> usize {
        match self.hidden_at(offset) {
            Some(r) if forward => r.end,
            Some(r) => {
                let text = self.doc.text();
                let mut p = r.start;
                if text[..p].ends_with('\n') {
                    p -= 1;
                }
                if text[..p].ends_with('\r') {
                    p -= 1;
                }
                p
            }
            None => offset,
        }
    }

    /// Heading level if `row` is a heading line.
    fn heading_level(&self, row: usize) -> Option<usize> {
        let start = self.line_starts[row];
        let hs = self.doc.headings();
        let i = hs.partition_point(|h| h.range.start < start);
        hs.get(i)
            .filter(|h| h.range.start == start)
            .map(|h| h.level)
    }

    /// Text size multiplier for a row (headings and titles are larger).
    fn row_scale(&self, row: usize) -> f32 {
        self.row_meta.get(row).map_or(1.0, |m| m.kind.scale())
    }

    fn row_kind(&self, row: usize) -> RowKind {
        self.row_meta
            .get(row)
            .map_or(RowKind::Body, |m| m.kind.clone())
    }

    /// Horizontal indent of a row (outline depth).
    fn row_indent(&self, row: usize) -> Pixels {
        if self.mode != EditorMode::Full {
            return px(0.);
        }
        indent_step() * self.row_meta.get(row).map_or(0, |m| m.depth) as f32
    }

    /// Wrap width available to a row after its indent.
    fn row_wrap_width(&self, row: usize, wrap_width: Option<Pixels>) -> Option<Pixels> {
        wrap_width.map(|w| (w - self.row_indent(row)).max(px(80.)))
    }

    fn wrap_key(&self, row: usize, scale: f32, wrap_width: Option<Pixels>) -> u64 {
        let mut h = DefaultHasher::new();
        self.row_meta.get(row).map(|m| m.hash).hash(&mut h);
        scale.to_bits().hash(&mut h);
        wrap_width.map(|w| f32::from(w).to_bits()).hash(&mut h);
        h.finish()
    }

    /// Whether the cursor is editing `d`, so its raw markup should show.
    fn revealed(&self, d: &Decoration) -> bool {
        let head = self.cursor_offset();
        let sel = &self.selected_range;
        let on_line = || {
            let row = self.row_for_offset(d.range.start);
            self.line_range(row).contains(&head) || self.line_range(row).end == head
        };
        match d.kind {
            // Whole-line markup shows while the cursor is anywhere on that line.
            DecoKind::HeadingStars | DecoKind::Title => on_line(),
            _ => {
                (d.range.start <= head && head <= d.range.end)
                    || (!sel.is_empty() && sel.start < d.range.end && sel.end > d.range.start)
            }
        }
    }

    /// Decorations that affect inline text on the line `range`.
    fn inline_decorations(&self, range: &Range<usize>) -> &[Decoration] {
        let decos = self.doc.decorations();
        let i = decos.partition_point(|d| d.range.start < range.start);
        let j = decos.partition_point(|d| d.range.start <= range.end);
        &decos[i..j.max(i)]
    }

    /// What row `row` displays right now.
    fn row_display(&self, row: usize) -> RowDisplay {
        let range = self.line_range(row);
        if self.mode != EditorMode::Full {
            return RowDisplay::plain(self.doc.text(), range);
        }
        if let Some(label) = self.drawer_chip(range.start) {
            return RowDisplay::replaced_line(range, &label);
        }
        let decos = self.inline_decorations(&range);
        RowDisplay::build(self.doc.text(), range, decos, |d| self.revealed(d))
    }

    /// Drawers as (whole range, body range), cached per change.
    fn drawers(&self) -> &[(Range<usize>, Range<usize>)] {
        &self.drawer_cache
    }

    /// Drawers as (whole range, body range); the body is everything after the first line.
    fn compute_drawers(&self) -> Vec<(Range<usize>, Range<usize>)> {
        let text = self.doc.text();
        self.doc
            .decorations()
            .iter()
            .filter(|d| d.kind == DecoKind::Drawer)
            .map(|d| {
                let first_end = text[d.range.start..]
                    .find('\n')
                    .map_or(d.range.end, |i| d.range.start + i + 1)
                    .min(d.range.end);
                (d.range.clone(), first_end..d.range.end)
            })
            .collect()
    }

    /// The chip label shown in place of a drawer's first line starting at `line_start`.
    fn drawer_chip(&self, line_start: usize) -> Option<String> {
        let (range, body) = self
            .drawers()
            .iter()
            .find(|(r, _)| r.start == line_start)
            .cloned()?;
        let text = self.doc.text();
        let name = text[range.start..body.start].trim().trim_matches(':');
        let mut c = name.chars();
        let name: String = c.next().map_or(String::new(), |f| {
            f.to_uppercase()
                .chain(c.flat_map(char::to_lowercase))
                .collect()
        });
        // Entries, not counting the :END: line.
        let entries = text[body]
            .lines()
            .filter(|l| !l.trim().eq_ignore_ascii_case(":end:") && !l.trim().is_empty())
            .count();
        let open = self.open_drawers.contains(&range.start);
        Some(format!(
            "{} {name} · {entries}",
            if open { "▾" } else { "▸" }
        ))
    }

    /// Expand the drawer whose hidden body contains `offset`. Returns whether anything changed.
    fn reveal_drawer(&mut self, offset: usize) -> bool {
        let hit = self
            .drawers()
            .iter()
            .find(|(r, body)| body.contains(&offset) && !self.open_drawers.contains(&r.start))
            .cloned();
        match hit {
            Some((r, _)) => {
                self.open_drawers.insert(r.start);
                true
            }
            None => false,
        }
    }

    fn toggle_drawer(&mut self, start: usize, cx: &mut Context<Self>) {
        if !self.open_drawers.remove(&start) {
            self.open_drawers.insert(start);
        }
        self.folds_changed(cx);
    }

    /// Styled ranges in display coordinates: line style, Org syntax, inline markup, search
    /// matches, and the IME composition.
    fn row_styles(&self, row: usize, display: &RowDisplay) -> spans::Spans {
        let range = display.line.clone();
        let to_disp = |r: &Range<usize>| display.display_range(r.clone());
        let in_row = |r: &Range<usize>| r.start >= range.start && r.start <= range.end;
        let mut out: spans::Spans = vec![(
            0..display.text.len(),
            self.row_kind(row).base_style(&self.colors),
        )];

        let i = self
            .org_spans
            .partition_point(|(r, _)| r.start < range.start);
        out.extend(
            self.org_spans[i..]
                .iter()
                .take_while(|(r, _)| r.start <= range.end)
                .map(|(r, st)| (to_disp(r), *st)),
        );
        if self.mode == EditorMode::Full {
            for d in self.inline_decorations(&range) {
                if let Some(st) = preview::inline_style(&d.kind, &self.colors) {
                    out.push((to_disp(&d.range), st));
                }
            }
        }
        if let Some(search) = &self.search {
            let i = search.matches.partition_point(|m| m.start < range.start);
            for (j, m) in search.matches[i..]
                .iter()
                .enumerate()
                .take_while(|(_, m)| in_row(m))
            {
                let bg = if Some(i + j) == search.active {
                    self.colors.selection
                } else {
                    self.colors.accent_subtle
                };
                out.push((
                    to_disp(m),
                    spans::SpanStyle {
                        background: Some(bg),
                        ..Default::default()
                    },
                ));
            }
        }
        if let Some(marked) = self.marked_range.as_ref().filter(|m| in_row(m)) {
            out.push((
                to_disp(marked),
                spans::SpanStyle {
                    underline: true,
                    ..Default::default()
                },
            ));
        }
        out
    }

    /// Lay out one row: its display text, styled and wrapped.
    fn shape_row(
        &self,
        row: usize,
        metrics: &Metrics,
        window: &mut Window,
    ) -> (WrappedLine, RowDisplay) {
        let display = self.row_display(row);
        let styles = self.row_styles(row, &display);
        let kind = self.row_kind(row);
        let mut font = metrics.font.clone();
        if kind.monospace() {
            font.family = metrics.mono_family.clone();
        }
        let base = TextRun {
            len: 0,
            font,
            color: metrics.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs =
            spans::runs_for_range(&base, &styles, 0..display.text.len(), &metrics.mono_family);
        let text: SharedString = display.text.clone().into();
        let wrapped = window
            .text_system()
            .shape_text(
                text,
                metrics.font_size * kind.scale(),
                &runs,
                self.row_wrap_width(row, metrics.wrap_width),
                None,
            )
            .ok()
            .and_then(|mut lines| lines.pop())
            .unwrap_or_default();
        (wrapped, display)
    }

    /// Make sure layout row `i` is shaped (rows far from the viewport are shaped lazily).
    fn ensure_shaped(&mut self, i: usize, window: &mut Window) {
        let (Some(layout), Some(metrics)) = (self.layout.as_ref(), self.metrics.clone()) else {
            return;
        };
        if layout.rows[i].wrapped.is_some() {
            return;
        }
        let (wrapped, display) = self.shape_row(layout.rows[i].row, &metrics, window);
        if let Some(layout) = self.layout.as_mut() {
            layout.rows[i].wrapped = Some(wrapped);
            layout.rows[i].display = display;
        }
    }

    // ---- selection ----------------------------------------------------------------------

    fn selection(&self) -> Selection {
        let r = &self.selected_range;
        if self.selection_reversed {
            Selection {
                anchor: r.end,
                head: r.start,
            }
        } else {
            Selection {
                anchor: r.start,
                head: r.end,
            }
        }
    }

    fn set_selection(&mut self, sel: Selection) {
        self.selected_range = sel.range();
        self.selection_reversed = sel.head < sel.anchor;
    }

    fn cursor_offset(&self) -> usize {
        self.selection().head
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.doc.seal_undo_group();
        self.set_selection(Selection::cursor(offset));
        self.autoscroll = true;
        self.restart_blink(cx);
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let anchor = self.selection().anchor;
        self.set_selection(Selection {
            anchor,
            head: offset,
        });
        self.autoscroll = true;
        self.restart_blink(cx);
        cx.notify();
    }

    fn cursor_in_table(&self) -> bool {
        self.row_kind(self.row_for_offset(self.cursor_offset())) == RowKind::Table
    }

    fn cursor_on_heading_line(&self) -> bool {
        self.heading_level(self.row_for_offset(self.cursor_offset()))
            .is_some()
    }

    fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_visible = true;
        self.blink_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(BLINK_INTERVAL).await;
                let alive = this.update(cx, |editor, cx| {
                    editor.cursor_visible = !editor.cursor_visible;
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        }));
    }

    // ---- geometry -----------------------------------------------------------------------

    fn layout_index_for_row(&self, row: usize) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        layout.rows.binary_search_by_key(&row, |r| r.row).ok()
    }

    /// Offset one visual line up/down from the cursor, keeping the preferred x position.
    fn vertical_target(&mut self, down: bool, window: &mut Window) -> usize {
        let cursor = self.cursor_offset();
        let row = self.row_for_offset(cursor);
        let Some(i) = self.layout_index_for_row(row) else {
            return cursor;
        };
        self.ensure_shaped(i, window);
        let layout = self.layout.as_ref().expect("layout exists");
        let r = &layout.rows[i];
        let col = r.display.to_display(cursor);
        let pos = r
            .wrapped
            .as_ref()
            .and_then(|w| w.position_for_index(col, r.line_height))
            .unwrap_or_default();
        let goal_x = *self.goal_x.get_or_insert(pos.x);
        let visual = (pos.y / r.line_height).round() as usize;

        let (target_i, target_visual) = if down {
            if visual + 1 < r.visual_lines {
                (i, Some(visual + 1))
            } else if i + 1 < layout.rows.len() {
                (i + 1, Some(0))
            } else {
                return self.doc.text().len();
            }
        } else if visual > 0 {
            (i, Some(visual - 1))
        } else if i > 0 {
            (i - 1, None) // last visual line of the previous row
        } else {
            return 0;
        };
        self.ensure_shaped(target_i, window);
        let layout = self.layout.as_ref().expect("layout exists");
        let t = &layout.rows[target_i];
        let Some(wrapped) = t.wrapped.as_ref() else {
            return cursor;
        };
        let lines = wrapped.wrap_boundaries().len() + 1;
        let v = target_visual.unwrap_or(lines - 1).min(lines - 1);
        let y = t.line_height * v as f32 + t.line_height / 2.;
        let col = match wrapped.closest_index_for_position(point(goal_x, y), t.line_height) {
            Ok(i) | Err(i) => i,
        };
        t.display.to_buffer(col)
    }

    /// The layout row and display index under a window position.
    fn hit_test(&mut self, position: Point<Pixels>, window: &mut Window) -> Option<(usize, usize)> {
        let layout = self.layout.as_ref()?;
        if layout.rows.is_empty() {
            return None;
        }
        let y = (position.y - layout.bounds.top()).max(px(0.));
        let i = layout
            .rows
            .partition_point(|r| r.top + r.height() <= y)
            .min(layout.rows.len() - 1);
        self.ensure_shaped(i, window);
        let layout = self.layout.as_ref().expect("layout exists");
        let r = &layout.rows[i];
        if y >= r.top + r.height() {
            return Some((i, r.display.text.len()));
        }
        let x = position.x - layout.text_left - r.indent;
        let local = point(x.max(px(0.)), (y - r.top).max(px(0.)));
        let col = r.wrapped.as_ref().map_or(0, |w| {
            match w.closest_index_for_position(local, r.line_height) {
                Ok(i) | Err(i) => i,
            }
        });
        Some((i, col))
    }

    /// Document offset at a window position.
    fn index_for_position(&mut self, position: Point<Pixels>, window: &mut Window) -> usize {
        match self.hit_test(position, window) {
            Some((i, col)) => {
                let layout = self.layout.as_ref().expect("layout exists");
                layout.rows[i].display.to_buffer(col)
            }
            None => 0,
        }
    }

    /// Window-space rectangle of the character range, for IME candidate windows.
    fn bounds_for_offsets(&self, range: Range<usize>) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        let row = self.row_for_offset(range.start);
        let r = &layout.rows[self.layout_index_for_row(row)?];
        let wrapped = r.wrapped.as_ref()?;
        let end = range.end.min(self.line_range(row).end);
        let disp = r.display.display_range(range.start..end);
        let p0 = wrapped.position_for_index(disp.start, r.line_height)?;
        let p1 = wrapped
            .position_for_index(disp.end, r.line_height)
            .unwrap_or(p0);
        let origin = point(layout.text_left + r.indent, layout.bounds.top() + r.top);
        Some(Bounds::from_corners(
            origin + p0,
            origin + point(p1.x.max(p0.x), p0.y + r.line_height),
        ))
    }

    // ---- search -------------------------------------------------------------------------

    /// Highlight matches of `query` and select the first one at or after the cursor.
    pub fn set_search_query(&mut self, query: &str, cx: &mut Context<Self>) {
        if query.is_empty() {
            self.search = None;
            cx.notify();
            return;
        }
        let matches = search::find_all(self.doc.text(), query);
        let from = self.selected_range.start;
        let active = (!matches.is_empty())
            .then(|| matches.iter().position(|m| m.start >= from).unwrap_or(0));
        self.search = Some(SearchState {
            query: query.to_string(),
            matches,
            active,
        });
        self.select_active_match(cx);
    }

    pub fn clear_search(&mut self, cx: &mut Context<Self>) {
        self.search = None;
        cx.notify();
    }

    /// (1-based active match, total matches)
    pub fn search_status(&self) -> (Option<usize>, usize) {
        self.search
            .as_ref()
            .map_or((None, 0), |s| (s.active.map(|i| i + 1), s.matches.len()))
    }

    pub fn search_next(&mut self, back: bool, cx: &mut Context<Self>) {
        let Some(s) = self.search.as_mut() else {
            return;
        };
        let n = s.matches.len();
        if n == 0 {
            return;
        }
        s.active = Some(match s.active {
            Some(i) if back => (i + n - 1) % n,
            Some(i) => (i + 1) % n,
            None => 0,
        });
        self.select_active_match(cx);
    }

    /// Replace the active match (if selected) and move to the next one.
    pub fn replace_active(&mut self, replacement: &str, cx: &mut Context<Self>) {
        let Some(m) = self
            .search
            .as_ref()
            .and_then(|s| s.active.map(|i| s.matches[i].clone()))
        else {
            return;
        };
        self.doc.seal_undo_group();
        self.edit(m, replacement, EditOrigin::Other, cx);
        self.doc.seal_undo_group();
        let from = self.cursor_offset();
        if let Some(s) = self.search.as_mut() {
            s.active = (!s.matches.is_empty())
                .then(|| s.matches.iter().position(|m| m.start >= from).unwrap_or(0));
        }
        self.select_active_match(cx);
    }

    /// Replace every match as a single undo step. Returns how many were replaced.
    pub fn replace_all(&mut self, replacement: &str, cx: &mut Context<Self>) -> usize {
        let Some(matches) = self.search.as_ref().map(|s| s.matches.clone()) else {
            return 0;
        };
        if matches.is_empty() {
            return 0;
        }
        let tx = search::replace_all(&matches, replacement);
        let before = self.selection();
        let after = before.map(&tx);
        self.doc.apply(tx, before, after, EditOrigin::Other);
        self.doc.seal_undo_group();
        self.set_selection(after);
        self.after_change(cx);
        matches.len()
    }

    fn refresh_search(&mut self) {
        let Some(s) = self.search.as_mut() else {
            return;
        };
        s.matches = search::find_all(self.doc.text(), &s.query);
        s.active = s.active.filter(|&i| i < s.matches.len());
    }

    fn select_active_match(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self
            .search
            .as_ref()
            .and_then(|s| s.active.map(|i| s.matches[i].clone()))
        else {
            cx.notify();
            return;
        };
        let folded = self
            .folds
            .reveal(m.start, self.doc.headings(), self.doc.text());
        if folded | self.reveal_drawer(m.start) {
            self.recompute_lines();
        }
        self.doc.seal_undo_group();
        self.set_selection(Selection {
            anchor: m.start,
            head: m.end,
        });
        self.autoscroll = true;
        cx.notify();
    }

    // ---- folding ------------------------------------------------------------------------

    /// The heading whose section contains the cursor (innermost).
    fn heading_at_cursor(&self) -> Option<aster_org::HeadingInfo> {
        let at = self.cursor_offset();
        let len = self.doc.text().len();
        self.doc
            .headings()
            .iter()
            .rev()
            .find(|h| h.range.start <= at && (at < h.range.end || h.range.end == len))
            .cloned()
    }

    fn set_fold(&mut self, h: &aster_org::HeadingInfo, folded: bool, cx: &mut Context<Self>) {
        self.folds.set(h, self.doc.text(), folded);
        self.folds_changed(cx);
    }

    fn folds_changed(&mut self, cx: &mut Context<Self>) {
        self.recompute_lines();
        // Don't leave the cursor inside hidden text.
        let sel = self.selection();
        let fix = |p: usize| self.clamp_visible(p, false);
        let (anchor, head) = (fix(sel.anchor), fix(sel.head));
        self.set_selection(Selection { anchor, head });
        self.autoscroll = true;
        cx.notify();
    }

    fn toggle_fold_heading(&mut self, h: &aster_org::HeadingInfo, cx: &mut Context<Self>) {
        let folded = self.folds.is_folded(h.range.start);
        self.set_fold(h, !folded, cx);
    }

    // ---- actions ------------------------------------------------------------------------

    fn left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup_key(NavKey::Left, window, cx) {
            return;
        }
        self.goal_x = None;
        let to = if self.selected_range.is_empty() {
            let p = text::prev_grapheme(self.doc.text(), self.cursor_offset());
            self.clamp_visible(p, false)
        } else {
            self.selected_range.start
        };
        self.move_to(to, cx);
    }

    fn right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup_key(NavKey::Right, window, cx) {
            return;
        }
        self.goal_x = None;
        let to = if self.selected_range.is_empty() {
            let p = text::next_grapheme(self.doc.text(), self.cursor_offset());
            self.clamp_visible(p, true)
        } else {
            self.selected_range.end
        };
        self.move_to(to, cx);
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let p = text::prev_word_start(self.doc.text(), self.cursor_offset());
        self.move_to(self.clamp_visible(p, false), cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let p = text::next_word_end(self.doc.text(), self.cursor_offset());
        self.move_to(self.clamp_visible(p, true), cx);
    }

    fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup_key(NavKey::Up, window, cx) {
            return;
        }
        if self.mode == EditorMode::SingleLine {
            cx.emit(EditorEvent::Navigate { up: true });
            return;
        }
        if !self.selected_range.is_empty() && self.goal_x.is_none() {
            self.set_selection(Selection::cursor(self.selected_range.start));
        }
        let target = self.vertical_target(false, window);
        self.move_to(target, cx);
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup_key(NavKey::Down, window, cx) {
            return;
        }
        if self.mode == EditorMode::SingleLine {
            cx.emit(EditorEvent::Navigate { up: false });
            return;
        }
        if !self.selected_range.is_empty() && self.goal_x.is_none() {
            self.set_selection(Selection::cursor(self.selected_range.end));
        }
        let target = self.vertical_target(true, window);
        self.move_to(target, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let p = text::prev_grapheme(self.doc.text(), self.cursor_offset());
        self.select_to(self.clamp_visible(p, false), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let p = text::next_grapheme(self.doc.text(), self.cursor_offset());
        self.select_to(self.clamp_visible(p, true), cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let p = text::prev_word_start(self.doc.text(), self.cursor_offset());
        self.select_to(self.clamp_visible(p, false), cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let p = text::next_word_end(self.doc.text(), self.cursor_offset());
        self.select_to(self.clamp_visible(p, true), cx);
    }

    fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.vertical_target(false, window);
        self.select_to(target, cx);
    }

    fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.vertical_target(true, window);
        self.select_to(target, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.set_selection(Selection {
            anchor: 0,
            head: self.doc.text().len(),
        });
        self.restart_blink(cx);
        cx.notify();
    }

    fn line_start(&mut self, _: &LineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let row = self.row_for_offset(self.cursor_offset());
        self.move_to(self.line_range(row).start, cx);
    }

    fn line_end(&mut self, _: &LineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let row = self.row_for_offset(self.cursor_offset());
        self.move_to(self.line_range(row).end, cx);
    }

    fn select_line_start(&mut self, _: &SelectLineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let row = self.row_for_offset(self.cursor_offset());
        self.select_to(self.line_range(row).start, cx);
    }

    fn select_line_end(&mut self, _: &SelectLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let row = self.row_for_offset(self.cursor_offset());
        self.select_to(self.line_range(row).end, cx);
    }

    fn doc_start(&mut self, _: &DocStart, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.move_to(0, cx);
    }

    fn doc_end(&mut self, _: &DocEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let end = self.clamp_visible(self.doc.text().len(), false);
        self.move_to(end, cx);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        let range = if self.selected_range.is_empty() {
            let cursor = self.cursor_offset();
            text::prev_grapheme(self.doc.text(), cursor)..cursor
        } else {
            self.selected_range.clone()
        };
        self.edit(range, "", EditOrigin::Delete, cx);
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        let range = if self.selected_range.is_empty() {
            let cursor = self.cursor_offset();
            cursor..text::next_grapheme(self.doc.text(), cursor)
        } else {
            self.selected_range.clone()
        };
        self.edit(range, "", EditOrigin::Delete, cx);
    }

    fn delete_word_left(&mut self, _: &DeleteWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let range = if self.selected_range.is_empty() {
            let cursor = self.cursor_offset();
            text::prev_word_start(self.doc.text(), cursor)..cursor
        } else {
            self.selected_range.clone()
        };
        self.edit(range, "", EditOrigin::Other, cx);
    }

    fn delete_word_right(&mut self, _: &DeleteWordRight, _: &mut Window, cx: &mut Context<Self>) {
        let range = if self.selected_range.is_empty() {
            let cursor = self.cursor_offset();
            cursor..text::next_word_end(self.doc.text(), cursor)
        } else {
            self.selected_range.clone()
        };
        self.edit(range, "", EditOrigin::Other, cx);
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup_key(NavKey::Enter, window, cx) {
            return;
        }
        if self.mode == EditorMode::SingleLine {
            cx.emit(EditorEvent::Confirm { back: false });
            return;
        }
        // Preserve the file's line-ending style.
        let eol = if self.doc.text().contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        self.edit(self.selected_range.clone(), eol, EditOrigin::Insert, cx);
    }

    fn shift_enter(&mut self, _: &ShiftEnter, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == EditorMode::SingleLine {
            cx.emit(EditorEvent::Confirm { back: true });
        } else {
            self.newline(&Newline, window, cx);
        }
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup_key(NavKey::Escape, window, cx) {
            return;
        }
        cx.emit(EditorEvent::Cancel);
    }

    /// In a table, Tab aligns it and moves to the next cell. On a heading line it demotes the
    /// heading and its children; elsewhere it indents.
    fn tab(&mut self, _: &Tab, _: &mut Window, cx: &mut Context<Self>) {
        match self.mode {
            EditorMode::SingleLine => cx.propagate(),
            EditorMode::Full if self.cursor_in_table() => {
                let r = aster_doc::table::next_cell(&self.doc, self.selection(), false);
                self.apply_op(r, cx);
            }
            EditorMode::Full if self.cursor_on_heading_line() => {
                let r = ops::demote(&self.doc, self.selection(), true);
                self.apply_op(r, cx);
            }
            EditorMode::Full => {
                self.edit(self.selected_range.clone(), "  ", EditOrigin::Insert, cx);
            }
        }
    }

    /// On a heading line, Shift+Tab promotes the heading and its children.
    fn outdent(&mut self, _: &Outdent, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == EditorMode::SingleLine {
            cx.propagate();
        } else if self.cursor_in_table() {
            let r = aster_doc::table::next_cell(&self.doc, self.selection(), true);
            self.apply_op(r, cx);
        } else if self.cursor_on_heading_line() {
            let r = ops::promote(&self.doc, self.selection(), true);
            self.apply_op(r, cx);
        }
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(sel) = self.doc.undo() {
            self.set_selection(sel);
            self.after_change(cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(sel) = self.doc.redo() {
            self.set_selection(sel);
            self.after_change(cx);
        }
    }

    /// Check off a checkbox on the current line; otherwise advance the heading's TODO state.
    fn toggle_done(&mut self, _: &ToggleDone, _: &mut Window, cx: &mut Context<Self>) {
        let sel = self.selection();
        let r =
            ops::toggle_checkbox(&self.doc, sel).or_else(|| ops::cycle_todo(&self.doc, sel, true));
        self.apply_op(r, cx);
    }

    fn cycle_todo_backward(
        &mut self,
        _: &CycleTodoBackward,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let r = ops::cycle_todo(&self.doc, self.selection(), false);
        self.apply_op(r, cx);
    }

    fn insert_heading(&mut self, _: &InsertHeading, _: &mut Window, cx: &mut Context<Self>) {
        let r = ops::insert_heading(&self.doc, self.selection());
        self.apply_op(r, cx);
    }

    fn promote_subtree(&mut self, _: &PromoteSubtree, _: &mut Window, cx: &mut Context<Self>) {
        let r = ops::promote(&self.doc, self.selection(), true);
        self.apply_op(r, cx);
    }

    fn demote_subtree(&mut self, _: &DemoteSubtree, _: &mut Window, cx: &mut Context<Self>) {
        let r = ops::demote(&self.doc, self.selection(), true);
        self.apply_op(r, cx);
    }

    fn promote_heading(&mut self, _: &PromoteHeading, _: &mut Window, cx: &mut Context<Self>) {
        let r = ops::promote(&self.doc, self.selection(), false);
        self.apply_op(r, cx);
    }

    fn demote_heading(&mut self, _: &DemoteHeading, _: &mut Window, cx: &mut Context<Self>) {
        let r = ops::demote(&self.doc, self.selection(), false);
        self.apply_op(r, cx);
    }

    fn move_subtree_up(&mut self, _: &MoveSubtreeUp, _: &mut Window, cx: &mut Context<Self>) {
        let r = ops::move_subtree(&self.doc, self.selection(), true);
        self.apply_op(r, cx);
    }

    fn move_subtree_down(&mut self, _: &MoveSubtreeDown, _: &mut Window, cx: &mut Context<Self>) {
        let r = ops::move_subtree(&self.doc, self.selection(), false);
        self.apply_op(r, cx);
    }

    fn open_heading_menu_action(
        &mut self,
        _: &OpenHeadingMenu,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(h) = self.heading_at_cursor() {
            let at = self.cursor_offset();
            let anchor = self.anchor_below(at..at);
            self.open_heading_menu(h.range.start, anchor, cx);
        }
    }

    fn toggle_fold(&mut self, _: &ToggleFold, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(h) = self.heading_at_cursor() {
            self.toggle_fold_heading(&h, cx);
        }
    }

    fn fold(&mut self, _: &Fold, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(h) = self.heading_at_cursor() {
            self.set_fold(&h, true, cx);
        }
    }

    fn unfold(&mut self, _: &Unfold, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(h) = self.heading_at_cursor() {
            self.set_fold(&h, false, cx);
        }
    }

    fn fold_all(&mut self, _: &FoldAll, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode != EditorMode::Full {
            return;
        }
        self.folds.fold_all(self.doc.headings(), self.doc.text());
        self.folds_changed(cx);
    }

    fn unfold_all(&mut self, _: &UnfoldAll, _: &mut Window, cx: &mut Context<Self>) {
        self.folds.unfold_all();
        self.folds_changed(cx);
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.edit(self.selected_range.clone(), &text, EditOrigin::Other, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.doc.text()[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.doc.text()[self.selected_range.clone()].to_string(),
            ));
            self.edit(self.selected_range.clone(), "", EditOrigin::Other, cx);
        }
    }

    // ---- mouse --------------------------------------------------------------------------

    /// Handle clicks on live-preview widgets: fold arrows, checkboxes, and Cmd/Ctrl+click
    /// on links. Returns true when the click was consumed.
    fn click_widget(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((i, col)) = self.hit_test(event.position, window) else {
            return false;
        };
        let layout = self.layout.as_ref().expect("layout exists");
        let r = &layout.rows[i];
        let row = r.row;

        // Left of a heading's text (gutter or indent): click toggles the fold, drag moves it.
        if event.position.x < layout.text_left + r.indent && self.heading_level(row).is_some() {
            self.heading_drag = Some(HeadingDrag {
                heading: self.line_starts[row],
                origin: event.position,
                active: false,
                target: None,
            });
            return true;
        }

        // A drawer chip: expand / collapse.
        if self.drawer_chip(self.line_starts[row]).is_some() {
            let start = self.line_starts[row];
            self.toggle_drawer(start, cx);
            return true;
        }

        // Clickable metadata: TODO keyword, priority, tags, dates.
        let offset = r.display.to_buffer(col);
        let line_start = self.line_starts[row];
        if let Some(h) = self
            .doc
            .headings()
            .iter()
            .find(|h| h.range.start == line_start)
            .cloned()
        {
            let hit = |range: &Option<Range<usize>>| {
                range
                    .as_ref()
                    .is_some_and(|r| r.start <= offset && offset < r.end)
            };
            let start = h.range.start;
            if hit(&h.todo_range) {
                let anchor = self.anchor_below(h.todo_range.clone().unwrap());
                self.open_todo_menu(start, anchor, cx);
                return true;
            }
            let priority = aster_doc::meta::priority_range(&self.doc, &h);
            if hit(&priority) {
                let anchor = self.anchor_below(priority.unwrap());
                self.open_priority_menu(start, anchor, cx);
                return true;
            }
            let tags = aster_doc::meta::tags_range(&self.doc, &h);
            if hit(&tags) {
                let anchor = self.anchor_below(tags.unwrap());
                self.open_tags_menu(start, anchor, cx);
                return true;
            }
        }
        let timestamp = self
            .doc
            .decorations()
            .iter()
            .find(|d| {
                d.kind == DecoKind::Timestamp && d.range.start <= offset && offset < d.range.end
            })
            .map(|d| d.range.clone());
        if let Some(range) = timestamp
            && !event.modifiers.shift
        {
            let anchor = self.anchor_below(range.clone());
            self.open_date_picker(popup::DateTarget::Timestamp(range), anchor, cx);
            return true;
        }

        // A rendered checkbox glyph: toggle it without moving the cursor.
        let glyph = r
            .display
            .segments
            .iter()
            .find(|s| s.replaced && s.disp.start <= col && col <= s.disp.end)
            .map(|s| s.buf.clone());
        if let Some(buf) = glyph
            && self
                .doc
                .decorations()
                .iter()
                .any(|d| d.range == buf && matches!(d.kind, DecoKind::Checkbox { .. }))
        {
            let result = ops::toggle_checkbox(&self.doc, Selection::cursor(buf.start));
            if let Some(result) = result {
                let sel = self.selection();
                let before = sel;
                let after = sel.map(&result.tx);
                self.doc.apply(result.tx, before, after, EditOrigin::Other);
                self.doc.seal_undo_group();
                self.set_selection(after);
                self.after_change(cx);
            }
            return true;
        }

        // Cmd/Ctrl+click follows a link.
        if event.modifiers.secondary() {
            let offset = r.display.to_buffer(col);
            let target = self.doc.decorations().iter().find_map(|d| match &d.kind {
                DecoKind::Link { target, .. }
                    if d.range.start <= offset && offset <= d.range.end =>
                {
                    Some(target.clone())
                }
                _ => None,
            });
            if let Some(target) = target {
                self.follow_link(&target, cx);
                return true;
            }
        }
        false
    }

    /// Follow a link: jump to headings and IDs in this document, or ask the host to open it.
    fn follow_link(&mut self, target: &str, cx: &mut Context<Self>) {
        let text = self.doc.text();
        let dest = if let Some(title) = target.strip_prefix('*') {
            self.doc
                .headings()
                .iter()
                .find(|h| h.title == title.trim())
                .map(|h| h.range.start)
        } else if let Some(id) = target.strip_prefix("id:") {
            text.match_indices(":ID:")
                .find(|(i, _)| {
                    let line_end = text[*i..].find('\n').map_or(text.len(), |e| i + e);
                    text[i + 4..line_end].trim() == id.trim()
                })
                .and_then(|(i, _)| {
                    self.doc
                        .headings()
                        .iter()
                        .rev()
                        .find(|h| h.range.start <= i)
                        .map(|h| h.range.start)
                })
        } else {
            None
        };
        match dest {
            Some(at) => {
                if self.folds.reveal(at, self.doc.headings(), self.doc.text()) {
                    self.recompute_lines();
                }
                self.move_to(at, cx);
            }
            // A heading that isn't in this file: nothing to do. An ID may be in another file.
            None if target.starts_with('*') => {}
            None => cx.emit(EditorEvent::OpenLink(target.to_string())),
        }
    }

    /// Range for a click with the given granularity at `offset`.
    fn granular_range(&self, granularity: Granularity, offset: usize) -> Range<usize> {
        match granularity {
            Granularity::Char => offset..offset,
            Granularity::Word => text::word_at(self.doc.text(), offset),
            Granularity::Line => {
                let row = self.row_for_offset(offset);
                let start = self.line_starts[row];
                let end = self
                    .line_starts
                    .get(row + 1)
                    .copied()
                    .unwrap_or(self.doc.text().len());
                start..end
            }
        }
    }

    /// Right-click on a heading line opens the heading menu.
    fn on_right_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.index_for_position(event.position, window);
        let row = self.row_for_offset(offset);
        let start = self.line_starts[row];
        if self.heading_level(row).is_some() {
            self.open_heading_menu(start, event.position, cx);
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_popup(window, cx);
        window.focus(&self.focus_handle);
        self.goal_x = None;

        if self.mode == EditorMode::Full && self.click_widget(event, window, cx) {
            return;
        }

        let offset = self.index_for_position(event.position, window);
        let granularity = match event.click_count {
            0 | 1 => Granularity::Char,
            2 => Granularity::Word,
            _ => Granularity::Line,
        };
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(offset, cx);
            self.drag = (
                Granularity::Char,
                self.selection().anchor..self.selection().anchor,
            );
        } else {
            let r = self.granular_range(granularity, offset);
            self.doc.seal_undo_group();
            self.set_selection(Selection {
                anchor: r.start,
                head: r.end,
            });
            self.drag = (granularity, r);
            self.restart_blink(cx);
            cx.notify();
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = false;
        let Some(drag) = self.heading_drag.take() else {
            return;
        };
        if !drag.active {
            // A plain click in the gutter.
            if let Some(h) = aster_doc::meta::heading(&self.doc, drag.heading).cloned() {
                self.toggle_fold_heading(&h, cx);
            }
            return;
        }
        if let Some((target, ..)) = drag.target {
            let r = aster_doc::meta::move_subtree_to(&self.doc, drag.heading, target);
            self.apply_op(r, cx);
        }
        cx.notify();
    }

    /// Where a heading dragged to `pos` would land: before the first visible heading whose
    /// middle is below the pointer (or at the end), at a level chosen by the pointer's x.
    fn drop_target(
        &self,
        dragged: usize,
        pos: Point<Pixels>,
    ) -> Option<(aster_doc::meta::DropTarget, Pixels, Pixels)> {
        let layout = self.layout.as_ref()?;
        let y = pos.y - layout.bounds.top();
        let headings = self.doc.headings();
        let dragged_h = aster_doc::meta::heading(&self.doc, dragged)?;
        let heading_rows: Vec<(&RowLayout, &aster_org::HeadingInfo)> = layout
            .rows
            .iter()
            .filter_map(|r| {
                let start = self.line_starts[r.row];
                headings
                    .iter()
                    .find(|h| h.range.start == start)
                    .map(|h| (r, h))
            })
            .collect();
        let next = heading_rows
            .iter()
            .position(|(r, _)| r.top + r.text_height() / 2. > y);
        let (before, line_y) = match next {
            Some(i) => (Some(heading_rows[i].1.range.start), heading_rows[i].0.top),
            None => (
                None,
                layout.rows.last().map_or(px(0.), |r| r.top + r.height()),
            ),
        };
        // The heading just above the drop line limits how deep the drop can nest.
        let above = match next {
            Some(0) => None,
            Some(i) => Some(heading_rows[i - 1].1),
            None => heading_rows.last().map(|(_, h)| *h),
        };
        let max_level = above.map_or(1, |h| h.level + 1);
        let step = indent_step();
        let from_x = ((pos.x - layout.text_left) / step).floor().max(0.) as usize + 1;
        let level = from_x.clamp(1, max_level);
        // Dropping inside the dragged subtree is not possible.
        if let Some(b) = before
            && b > dragged_h.range.start
            && b < dragged_h.range.end
        {
            return None;
        }
        let x = layout.text_left + step * (level - 1) as f32;
        Some((
            aster_doc::meta::DropTarget { before, level },
            layout.bounds.top() + line_y,
            x,
        ))
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(drag) = self.heading_drag.clone() {
            let moved =
                (event.position.x - drag.origin.x).abs() + (event.position.y - drag.origin.y).abs();
            if drag.active || moved > px(4.) {
                let target = self.drop_target(drag.heading, event.position);
                self.heading_drag = Some(HeadingDrag {
                    active: true,
                    target,
                    ..drag
                });
                cx.notify();
            }
            return;
        }
        if !self.is_selecting {
            return;
        }
        let offset = self.index_for_position(event.position, window);
        let (granularity, origin) = self.drag.clone();
        let here = self.granular_range(granularity, offset);
        let sel = if here.start < origin.start {
            Selection {
                anchor: origin.end,
                head: here.start,
            }
        } else {
            Selection {
                anchor: origin.start,
                head: here.end.max(origin.end),
            }
        };
        self.set_selection(sel);
        self.autoscroll = true;
        cx.notify();
    }
}

impl Render for OrgEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let base = div()
            .id(if self.mode == EditorMode::Full {
                "org-editor"
            } else {
                "org-input"
            })
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::delete_word_right))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::line_start))
            .on_action(cx.listener(Self::line_end))
            .on_action(cx.listener(Self::select_line_start))
            .on_action(cx.listener(Self::select_line_end))
            .on_action(cx.listener(Self::doc_start))
            .on_action(cx.listener(Self::doc_end))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::shift_enter))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::tab))
            .on_action(cx.listener(Self::outdent))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move));

        match self.mode {
            EditorMode::SingleLine => base
                .w_full()
                .overflow_hidden()
                .child(EditorElement::new(cx.entity())),
            EditorMode::Full => base
                .on_action(cx.listener(Self::toggle_done))
                .on_action(cx.listener(Self::cycle_todo_backward))
                .on_action(cx.listener(Self::insert_heading))
                .on_action(cx.listener(Self::promote_subtree))
                .on_action(cx.listener(Self::demote_subtree))
                .on_action(cx.listener(Self::promote_heading))
                .on_action(cx.listener(Self::demote_heading))
                .on_action(cx.listener(Self::move_subtree_up))
                .on_action(cx.listener(Self::move_subtree_down))
                .on_action(cx.listener(Self::toggle_fold))
                .on_action(cx.listener(Self::open_heading_menu_action))
                .on_mouse_down(MouseButton::Right, cx.listener(Self::on_right_mouse_down))
                .children(self.render_popup(cx))
                .on_action(cx.listener(Self::fold))
                .on_action(cx.listener(Self::unfold))
                .on_action(cx.listener(Self::fold_all))
                .on_action(cx.listener(Self::unfold_all))
                .track_scroll(&self.scroll_handle)
                .overflow_y_scroll()
                .size_full()
                .pl_3()
                .pr_8()
                .py_6()
                .child(EditorElement::new(cx.entity())),
        }
    }
}

impl Focusable for OrgEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Whether `#+STARTUP:` asks for the file to open folded (`overview` / `fold`).
fn startup_folded(text: &str) -> bool {
    text.lines()
        .filter_map(|l| {
            let l = l.trim_start();
            let (key, value) = l.split_once(':')?;
            key.eq_ignore_ascii_case("#+startup").then_some(value)
        })
        .flat_map(str::split_whitespace)
        .fold(false, |folded, opt| match opt {
            "overview" | "fold" => true,
            "showall" | "showeverything" | "nofold" => false,
            _ => folded,
        })
}
