//! Optional Vim-style modal editing for the document editor (single-line inputs never use it).
//!
//! The mode lives on each editor and is exposed to the keymap as a key context
//! (`vim_normal`, `vim_insert`, `vim_visual`), so Vim keys are ordinary GPUI bindings layered
//! over the standard ones. In Normal and Visual mode typed text is ignored.

use gpui::{App, ClipboardItem, Context, Global, KeyBinding, Window, actions};

use crate::{EditOrigin, EditorMode, OrgEditor, Selection, text};

actions!(
    vim,
    [
        NormalMode,
        InsertMode,
        InsertAfter,
        InsertLineStart,
        InsertLineEnd,
        OpenLineBelow,
        OpenLineAbove,
        VisualMode,
        VisualLineMode,
        Left,
        Right,
        Up,
        Down,
        WordForward,
        WordBack,
        WordEnd,
        LineStart,
        FirstNonBlank,
        LineEnd,
        DocStart,
        DocEnd,
        DeleteChar,
        DeleteCharBefore,
        DeleteLine,
        ChangeLine,
        YankLine,
        DeleteWord,
        ChangeWord,
        DeleteToEnd,
        ChangeToEnd,
        PasteAfter,
        PasteBefore,
        JoinLines,
        VisualDelete,
        VisualYank,
        VisualChange,
    ]
);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VimMode {
    #[default]
    Normal,
    Insert,
    Visual,
    VisualLine,
}

impl VimMode {
    pub fn label(self) -> &'static str {
        match self {
            VimMode::Normal => "NORMAL",
            VimMode::Insert => "INSERT",
            VimMode::Visual => "VISUAL",
            VimMode::VisualLine => "VISUAL LINE",
        }
    }

    fn is_visual(self) -> bool {
        matches!(self, VimMode::Visual | VimMode::VisualLine)
    }
}

/// Per-editor Vim state.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct VimState {
    pub mode: VimMode,
    /// Visual mode: where it started, and the moving end (character offsets, inclusive).
    anchor: usize,
    head: usize,
}

/// Whether Vim mode is on (the app's keymap setting).
#[derive(Debug, Default)]
pub struct VimSettings {
    pub enabled: bool,
}

impl Global for VimSettings {}

pub fn set_enabled(enabled: bool, cx: &mut App) {
    cx.set_global(VimSettings { enabled });
    cx.refresh_windows();
}

/// Vim bindings for the editor (the app adds `/`, `n`, `:` and friends).
pub fn bindings() -> Vec<KeyBinding> {
    let n = Some("OrgEditor && vim_normal");
    let v = Some("OrgEditor && vim_visual");
    let i = Some("OrgEditor && vim_insert");
    let mut b = Vec::new();
    // Motions work the same in Normal and Visual mode.
    for ctx in [n, v] {
        b.extend([
            KeyBinding::new("h", Left, ctx),
            KeyBinding::new("l", Right, ctx),
            KeyBinding::new("j", Down, ctx),
            KeyBinding::new("k", Up, ctx),
            KeyBinding::new("left", Left, ctx),
            KeyBinding::new("right", Right, ctx),
            KeyBinding::new("down", Down, ctx),
            KeyBinding::new("up", Up, ctx),
            KeyBinding::new("w", WordForward, ctx),
            KeyBinding::new("b", WordBack, ctx),
            KeyBinding::new("e", WordEnd, ctx),
            KeyBinding::new("0", LineStart, ctx),
            KeyBinding::new("^", FirstNonBlank, ctx),
            KeyBinding::new("shift-6", FirstNonBlank, ctx),
            KeyBinding::new("$", LineEnd, ctx),
            KeyBinding::new("shift-4", LineEnd, ctx),
            KeyBinding::new("g g", DocStart, ctx),
            KeyBinding::new("shift-g", DocEnd, ctx),
        ]);
    }
    b.extend([
        // Entering Insert mode
        KeyBinding::new("i", InsertMode, n),
        KeyBinding::new("a", InsertAfter, n),
        KeyBinding::new("shift-i", InsertLineStart, n),
        KeyBinding::new("shift-a", InsertLineEnd, n),
        KeyBinding::new("o", OpenLineBelow, n),
        KeyBinding::new("shift-o", OpenLineAbove, n),
        KeyBinding::new("v", VisualMode, n),
        KeyBinding::new("shift-v", VisualLineMode, n),
        // Editing
        KeyBinding::new("x", DeleteChar, n),
        KeyBinding::new("shift-x", DeleteCharBefore, n),
        KeyBinding::new("d d", DeleteLine, n),
        KeyBinding::new("c c", ChangeLine, n),
        KeyBinding::new("y y", YankLine, n),
        KeyBinding::new("shift-y", YankLine, n),
        KeyBinding::new("d w", DeleteWord, n),
        KeyBinding::new("c w", ChangeWord, n),
        KeyBinding::new("d $", DeleteToEnd, n),
        KeyBinding::new("d shift-4", DeleteToEnd, n),
        KeyBinding::new("shift-d", DeleteToEnd, n),
        KeyBinding::new("c $", ChangeToEnd, n),
        KeyBinding::new("c shift-4", ChangeToEnd, n),
        KeyBinding::new("shift-c", ChangeToEnd, n),
        KeyBinding::new("p", PasteAfter, n),
        KeyBinding::new("shift-p", PasteBefore, n),
        KeyBinding::new("shift-j", JoinLines, n),
        KeyBinding::new("u", crate::Undo, n),
        KeyBinding::new("ctrl-r", crate::Redo, n),
        KeyBinding::new("enter", Down, n),
        // Org: outline structure and folding (as in evil-org)
        KeyBinding::new("> >", crate::DemoteSubtree, n),
        KeyBinding::new("shift-. shift-.", crate::DemoteSubtree, n),
        KeyBinding::new("< <", crate::PromoteSubtree, n),
        KeyBinding::new("shift-, shift-,", crate::PromoteSubtree, n),
        KeyBinding::new("tab", crate::OrgCycle, n),
        KeyBinding::new("shift-tab", crate::OrgGlobalCycle, n),
        KeyBinding::new("z a", crate::ToggleFold, n),
        KeyBinding::new("z c", crate::Fold, n),
        KeyBinding::new("z o", crate::Unfold, n),
        KeyBinding::new("z shift-m", crate::FoldAll, n),
        KeyBinding::new("z shift-r", crate::UnfoldAll, n),
        KeyBinding::new("t", crate::ToggleDone, n),
        // Visual mode
        KeyBinding::new("escape", NormalMode, v),
        KeyBinding::new("v", VisualMode, v),
        KeyBinding::new("shift-v", VisualLineMode, v),
        KeyBinding::new("d", VisualDelete, v),
        KeyBinding::new("x", VisualDelete, v),
        KeyBinding::new("y", VisualYank, v),
        KeyBinding::new("c", VisualChange, v),
        KeyBinding::new("s", VisualChange, v),
        KeyBinding::new(">", crate::DemoteSubtree, v),
        KeyBinding::new("shift-.", crate::DemoteSubtree, v),
        KeyBinding::new("<", crate::PromoteSubtree, v),
        KeyBinding::new("shift-,", crate::PromoteSubtree, v),
        // Insert mode
        KeyBinding::new("escape", NormalMode, i),
        KeyBinding::new("ctrl-[", NormalMode, i),
    ]);
    b
}

// ---- word motions (Vim's "word": letters/digits/_ runs, or punctuation runs) ----------------

fn class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

/// `w`: the start of the next word.
fn next_word_start(text: &str, pos: usize) -> usize {
    let mut chars = text[pos..].char_indices().peekable();
    let Some(&(_, first)) = chars.peek() else {
        return text.len();
    };
    let c0 = class(first);
    if c0 != 0 {
        while chars.peek().is_some_and(|&(_, c)| class(c) == c0) {
            chars.next();
        }
    }
    while chars.peek().is_some_and(|&(_, c)| class(c) == 0) {
        chars.next();
    }
    chars.peek().map_or(text.len(), |&(i, _)| pos + i)
}

/// `b`: the start of this or the previous word.
fn prev_word_start(text: &str, pos: usize) -> usize {
    let mut chars = text[..pos].char_indices().rev().peekable();
    while chars.peek().is_some_and(|&(_, c)| class(c) == 0) {
        chars.next();
    }
    let Some(&(mut start, first)) = chars.peek() else {
        return 0;
    };
    let c0 = class(first);
    while let Some(&(i, c)) = chars.peek() {
        if class(c) != c0 {
            break;
        }
        start = i;
        chars.next();
    }
    start
}

/// `e`: the last character of this or the next word.
fn word_end(text: &str, pos: usize) -> usize {
    let mut chars = text[pos..].char_indices().skip(1).peekable();
    while chars.peek().is_some_and(|&(_, c)| class(c) == 0) {
        chars.next();
    }
    let Some(&(mut end, first)) = chars.peek() else {
        return pos;
    };
    let c0 = class(first);
    while let Some(&(i, c)) = chars.peek() {
        if class(c) != c0 {
            break;
        }
        end = i;
        chars.next();
    }
    pos + end
}

impl OrgEditor {
    /// The Vim mode, when Vim keys are on for this editor.
    pub fn vim_mode(&self, cx: &App) -> Option<VimMode> {
        (self.mode == EditorMode::Full && cx.try_global::<VimSettings>().is_some_and(|v| v.enabled))
            .then_some(self.vim.mode)
    }

    /// Whether typed text should be ignored (Vim Normal/Visual mode).
    pub(crate) fn vim_blocks_text(&self, cx: &App) -> bool {
        self.vim_mode(cx).is_some_and(|m| m != VimMode::Insert)
    }

    fn vim_set_mode(&mut self, mode: VimMode, cx: &mut Context<Self>) {
        self.vim.mode = mode;
        self.doc.seal_undo_group();
        cx.notify();
    }

    /// The current line's range (without its line break).
    fn vim_line(&self) -> std::ops::Range<usize> {
        self.line_range(self.row_for_offset(self.cursor_offset()))
    }

    fn vim_first_non_blank(&self, line: std::ops::Range<usize>) -> usize {
        let text = &self.doc.text()[line.clone()];
        line.start + (text.len() - text.trim_start().len())
    }

    /// Run a motion: in Visual mode it moves the selection's moving end.
    fn vim_motion(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>),
    ) {
        let visual = self.vim.mode.is_visual();
        if visual {
            self.set_selection(Selection::cursor(self.vim.head));
        }
        f(self, window, cx);
        if visual {
            self.vim.head = self.cursor_offset();
            self.vim_select(cx);
        }
    }

    /// Select the Visual range (inclusive of the character under each end, or whole lines).
    fn vim_select(&mut self, cx: &mut Context<Self>) {
        let (a, h) = (self.vim.anchor, self.vim.head);
        let range = self.vim_visual_range();
        self.set_selection(if h < a {
            Selection {
                anchor: range.end,
                head: range.start,
            }
        } else {
            Selection {
                anchor: range.start,
                head: range.end,
            }
        });
        self.autoscroll = true;
        cx.notify();
    }

    fn vim_visual_range(&self) -> std::ops::Range<usize> {
        let text = self.doc.text();
        let (lo, hi) = (
            self.vim.anchor.min(self.vim.head),
            self.vim.anchor.max(self.vim.head),
        );
        if self.vim.mode == VimMode::VisualLine {
            let start = self.line_range(self.row_for_offset(lo)).start;
            let last = self.row_for_offset(hi);
            let end = self
                .line_starts
                .get(last + 1)
                .copied()
                .unwrap_or(text.len());
            start..end
        } else {
            lo..text::next_grapheme(text, hi).max(hi).min(text.len())
        }
    }

    /// The last real line: in Vim, the empty "line" after a final line break doesn't exist.
    fn vim_last_row(&self) -> usize {
        let last = self.line_starts.len() - 1;
        if last > 0 && self.line_starts[last] == self.doc.text().len() {
            last - 1
        } else {
            last
        }
    }

    fn vim_yank(&self, s: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(s));
    }

    // ---- mode changes ----------------------------------------------------------------------

    fn vim_normal_mode(&mut self, _: &NormalMode, window: &mut Window, cx: &mut Context<Self>) {
        match self.vim.mode {
            VimMode::Insert => {
                // Escape closes a menu first (slash menu, completion), as it would anyway.
                if self.popup_key(crate::NavKey::Escape, window, cx) {
                    return;
                }
                let cursor = self.cursor_offset();
                let line = self.vim_line();
                if cursor > line.start {
                    self.move_to(text::prev_grapheme(self.doc.text(), cursor), cx);
                }
            }
            VimMode::Visual | VimMode::VisualLine => {
                self.move_to(self.vim.head, cx);
            }
            VimMode::Normal => {}
        }
        self.vim_set_mode(VimMode::Normal, cx);
    }

    fn vim_insert(&mut self, _: &InsertMode, _: &mut Window, cx: &mut Context<Self>) {
        self.vim_set_mode(VimMode::Insert, cx);
    }

    fn vim_insert_after(&mut self, _: &InsertAfter, _: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        if cursor < self.vim_line().end {
            self.move_to(text::next_grapheme(self.doc.text(), cursor), cx);
        }
        self.vim_set_mode(VimMode::Insert, cx);
    }

    fn vim_insert_line_start(
        &mut self,
        _: &InsertLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = self.vim_first_non_blank(self.vim_line());
        self.move_to(to, cx);
        self.vim_set_mode(VimMode::Insert, cx);
    }

    fn vim_insert_line_end(&mut self, _: &InsertLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.vim_line().end, cx);
        self.vim_set_mode(VimMode::Insert, cx);
    }

    fn vim_open_below(&mut self, _: &OpenLineBelow, _: &mut Window, cx: &mut Context<Self>) {
        let end = self.vim_line().end;
        self.edit(end..end, "\n", EditOrigin::Other, cx);
        self.vim_set_mode(VimMode::Insert, cx);
    }

    fn vim_open_above(&mut self, _: &OpenLineAbove, _: &mut Window, cx: &mut Context<Self>) {
        let start = self.vim_line().start;
        self.edit(start..start, "\n", EditOrigin::Other, cx);
        self.move_to(start, cx);
        self.vim_set_mode(VimMode::Insert, cx);
    }

    fn vim_visual(&mut self, _: &VisualMode, _: &mut Window, cx: &mut Context<Self>) {
        self.vim_enter_visual(VimMode::Visual, cx);
    }

    fn vim_visual_line(&mut self, _: &VisualLineMode, _: &mut Window, cx: &mut Context<Self>) {
        self.vim_enter_visual(VimMode::VisualLine, cx);
    }

    fn vim_enter_visual(&mut self, mode: VimMode, cx: &mut Context<Self>) {
        if self.vim.mode == mode {
            // Pressing v in Visual mode leaves it.
            self.move_to(self.vim.head, cx);
            self.vim_set_mode(VimMode::Normal, cx);
            return;
        }
        if !self.vim.mode.is_visual() {
            let cursor = self.cursor_offset();
            self.vim.anchor = cursor;
            self.vim.head = cursor;
        }
        self.vim_set_mode(mode, cx);
        self.vim_select(cx);
    }

    // ---- motions -----------------------------------------------------------------------

    fn vim_left(&mut self, _: &Left, w: &mut Window, cx: &mut Context<Self>) {
        self.vim_motion(w, cx, |e, _, cx| {
            let cursor = e.cursor_offset();
            if cursor > e.vim_line().start {
                e.goal_x = None;
                e.move_to(text::prev_grapheme(e.doc.text(), cursor), cx);
            }
        });
    }

    fn vim_right(&mut self, _: &Right, w: &mut Window, cx: &mut Context<Self>) {
        self.vim_motion(w, cx, |e, _, cx| {
            let cursor = e.cursor_offset();
            let next = text::next_grapheme(e.doc.text(), cursor);
            if next < e.vim_line().end {
                e.goal_x = None;
                e.move_to(next, cx);
            }
        });
    }

    fn vim_down(&mut self, _: &Down, w: &mut Window, cx: &mut Context<Self>) {
        self.vim_motion(w, cx, |e, w, cx| e.down(&crate::Down, w, cx));
    }

    fn vim_up(&mut self, _: &Up, w: &mut Window, cx: &mut Context<Self>) {
        self.vim_motion(w, cx, |e, w, cx| e.up(&crate::Up, w, cx));
    }

    fn vim_word_forward(&mut self, _: &WordForward, w: &mut Window, cx: &mut Context<Self>) {
        self.vim_motion(w, cx, |e, _, cx| {
            let to = next_word_start(e.doc.text(), e.cursor_offset());
            e.move_to(to, cx);
        });
    }

    fn vim_word_back(&mut self, _: &WordBack, w: &mut Window, cx: &mut Context<Self>) {
        self.vim_motion(w, cx, |e, _, cx| {
            let to = prev_word_start(e.doc.text(), e.cursor_offset());
            e.move_to(to, cx);
        });
    }

    fn vim_word_end(&mut self, _: &WordEnd, w: &mut Window, cx: &mut Context<Self>) {
        self.vim_motion(w, cx, |e, _, cx| {
            let to = word_end(e.doc.text(), e.cursor_offset());
            e.move_to(to, cx);
        });
    }

    fn vim_line_start(&mut self, _: &LineStart, w: &mut Window, cx: &mut Context<Self>) {
        self.vim_motion(w, cx, |e, _, cx| e.move_to(e.vim_line().start, cx));
    }

    fn vim_first_non_blank_action(
        &mut self,
        _: &FirstNonBlank,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.vim_motion(w, cx, |e, _, cx| {
            let to = e.vim_first_non_blank(e.vim_line());
            e.move_to(to, cx);
        });
    }

    fn vim_line_end(&mut self, _: &LineEnd, w: &mut Window, cx: &mut Context<Self>) {
        self.vim_motion(w, cx, |e, _, cx| {
            let line = e.vim_line();
            // On the last character, not past it.
            let to = if line.is_empty() {
                line.start
            } else {
                text::prev_grapheme(e.doc.text(), line.end)
            };
            e.move_to(to, cx);
        });
    }

    fn vim_doc_start(&mut self, _: &DocStart, w: &mut Window, cx: &mut Context<Self>) {
        self.vim_motion(w, cx, |e, _, cx| e.move_to(0, cx));
    }

    fn vim_doc_end(&mut self, _: &DocEnd, w: &mut Window, cx: &mut Context<Self>) {
        self.vim_motion(w, cx, |e, _, cx| {
            let to = e.line_range(e.vim_last_row()).start;
            e.move_to(to, cx);
        });
    }

    // ---- edits -------------------------------------------------------------------------

    fn vim_delete_range(&mut self, range: std::ops::Range<usize>, cx: &mut Context<Self>) {
        if range.is_empty() {
            return;
        }
        self.vim_yank(self.doc.text()[range.clone()].to_string(), cx);
        self.edit(range, "", EditOrigin::Other, cx);
    }

    fn vim_delete_char(&mut self, _: &DeleteChar, _: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        let line = self.vim_line();
        let end = text::next_grapheme(self.doc.text(), cursor).min(line.end);
        self.vim_delete_range(cursor..end, cx);
        // Stay on the line's last character when deleting at its end.
        let line = self.vim_line();
        if self.cursor_offset() >= line.end && line.end > line.start {
            self.move_to(text::prev_grapheme(self.doc.text(), line.end), cx);
        }
    }

    fn vim_delete_char_before(
        &mut self,
        _: &DeleteCharBefore,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cursor = self.cursor_offset();
        if cursor > self.vim_line().start {
            let start = text::prev_grapheme(self.doc.text(), cursor);
            self.vim_delete_range(start..cursor, cx);
        }
    }

    /// The current line including its line break (or the one before it, on the last line).
    fn vim_whole_line(&self) -> (std::ops::Range<usize>, String) {
        let text = self.doc.text();
        let row = self.row_for_offset(self.cursor_offset());
        let line = self.line_range(row);
        let content = format!("{}\n", &text[line.clone()]);
        let range = match self.line_starts.get(row + 1) {
            Some(&next) => line.start..next,
            None if line.start > 0 => {
                // Last line: take the break before it instead.
                let prev_end = self.line_range(row - 1).end;
                prev_end..line.end
            }
            None => line.start..line.end,
        };
        (range, content)
    }

    fn vim_delete_line(&mut self, _: &DeleteLine, _: &mut Window, cx: &mut Context<Self>) {
        let (range, content) = self.vim_whole_line();
        self.vim_yank(content, cx);
        self.edit(range, "", EditOrigin::Other, cx);
        let to = self.vim_first_non_blank(self.vim_line());
        self.move_to(to, cx);
    }

    fn vim_yank_line(&mut self, _: &YankLine, _: &mut Window, cx: &mut Context<Self>) {
        let (_, content) = self.vim_whole_line();
        self.vim_yank(content, cx);
    }

    fn vim_change_line(&mut self, _: &ChangeLine, _: &mut Window, cx: &mut Context<Self>) {
        let line = self.vim_line();
        let start = self.vim_first_non_blank(line.clone());
        self.vim_delete_range(start..line.end, cx);
        self.vim_set_mode(VimMode::Insert, cx);
    }

    fn vim_delete_word(&mut self, _: &DeleteWord, _: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        let end = next_word_start(self.doc.text(), cursor).min(self.vim_line().end.max(cursor + 1));
        let end = end.min(self.doc.text().len());
        self.vim_delete_range(cursor..end, cx);
    }

    fn vim_change_word(&mut self, _: &ChangeWord, _: &mut Window, cx: &mut Context<Self>) {
        // Vim's `cw` changes to the end of the word (like `ce`).
        let text = self.doc.text();
        let cursor = self.cursor_offset();
        let end = if text[cursor..].starts_with(char::is_whitespace) {
            next_word_start(text, cursor)
        } else {
            text::next_grapheme(text, word_end(text, cursor).max(cursor))
        };
        let end = end.min(self.vim_line().end);
        self.vim_delete_range(cursor..end, cx);
        self.vim_set_mode(VimMode::Insert, cx);
    }

    fn vim_delete_to_end(&mut self, _: &DeleteToEnd, _: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        self.vim_delete_range(cursor..self.vim_line().end, cx);
    }

    fn vim_change_to_end(&mut self, _: &ChangeToEnd, _: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        self.vim_delete_range(cursor..self.vim_line().end, cx);
        self.vim_set_mode(VimMode::Insert, cx);
    }

    fn vim_paste(&mut self, before: bool, cx: &mut Context<Self>) {
        let Some(clip) = cx.read_from_clipboard().and_then(|c| c.text()) else {
            return;
        };
        let line = self.vim_line();
        let phantom = line.is_empty() && line.start > 0 && line.start == self.doc.text().len();
        if let Some(body) = clip.strip_suffix('\n') {
            // Linewise: a new line below (or above) this one.
            if before || phantom {
                self.edit(
                    line.start..line.start,
                    &format!("{body}\n"),
                    EditOrigin::Other,
                    cx,
                );
                self.move_to(line.start, cx);
            } else {
                self.edit(
                    line.end..line.end,
                    &format!("\n{body}"),
                    EditOrigin::Other,
                    cx,
                );
                self.move_to(line.end + 1, cx);
            }
            let to = self.vim_first_non_blank(self.vim_line());
            self.move_to(to, cx);
        } else {
            let cursor = self.cursor_offset();
            let at = if before || cursor >= line.end {
                cursor
            } else {
                text::next_grapheme(self.doc.text(), cursor)
            };
            self.edit(at..at, &clip, EditOrigin::Other, cx);
            // On the last pasted character.
            let end = self.cursor_offset();
            if end > at {
                self.move_to(text::prev_grapheme(self.doc.text(), end), cx);
            }
        }
    }

    fn vim_paste_after(&mut self, _: &PasteAfter, _: &mut Window, cx: &mut Context<Self>) {
        self.vim_paste(false, cx);
    }

    fn vim_paste_before(&mut self, _: &PasteBefore, _: &mut Window, cx: &mut Context<Self>) {
        self.vim_paste(true, cx);
    }

    fn vim_join_lines(&mut self, _: &JoinLines, _: &mut Window, cx: &mut Context<Self>) {
        let text = self.doc.text();
        let line = self.vim_line();
        let row = self.row_for_offset(line.start);
        let Some(&next) = self.line_starts.get(row + 1) else {
            return;
        };
        let next_line = self.line_range(row + 1);
        let rest = &text[next_line.clone()];
        let skip = rest.len() - rest.trim_start().len();
        let joiner = if rest.trim().is_empty() || text[..line.end].ends_with(' ') {
            ""
        } else {
            " "
        };
        let _ = next;
        self.edit(
            line.end..next_line.start + skip,
            joiner,
            EditOrigin::Other,
            cx,
        );
        self.move_to(line.end, cx);
    }

    fn vim_visual_take(&mut self, cx: &mut Context<Self>) -> std::ops::Range<usize> {
        let range = self.vim_visual_range();
        let mut s = self.doc.text()[range.clone()].to_string();
        if self.vim.mode == VimMode::VisualLine && !s.ends_with('\n') {
            s.push('\n');
        }
        self.vim_yank(s, cx);
        range
    }

    fn vim_visual_delete(&mut self, _: &VisualDelete, _: &mut Window, cx: &mut Context<Self>) {
        let linewise = self.vim.mode == VimMode::VisualLine;
        let mut range = self.vim_visual_take(cx);
        if linewise && range.end == self.doc.text().len() && range.start > 0 {
            // The last line: remove the line break before it too.
            range.start -= 1;
        }
        self.edit(range, "", EditOrigin::Other, cx);
        self.vim_set_mode(VimMode::Normal, cx);
    }

    fn vim_visual_yank(&mut self, _: &VisualYank, _: &mut Window, cx: &mut Context<Self>) {
        let range = self.vim_visual_take(cx);
        self.move_to(range.start, cx);
        self.vim_set_mode(VimMode::Normal, cx);
    }

    fn vim_visual_change(&mut self, _: &VisualChange, _: &mut Window, cx: &mut Context<Self>) {
        let mut range = self.vim_visual_take(cx);
        if self.vim.mode == VimMode::VisualLine && self.doc.text()[..range.end].ends_with('\n') {
            range.end -= 1; // keep the line itself
        }
        self.edit(range, "", EditOrigin::Other, cx);
        self.vim_set_mode(VimMode::Insert, cx);
    }

    /// Register the Vim action handlers on the editor's element.
    pub(crate) fn vim_actions(
        &self,
        div: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        use gpui::InteractiveElement as _;
        div.on_action(cx.listener(Self::vim_normal_mode))
            .on_action(cx.listener(Self::vim_insert))
            .on_action(cx.listener(Self::vim_insert_after))
            .on_action(cx.listener(Self::vim_insert_line_start))
            .on_action(cx.listener(Self::vim_insert_line_end))
            .on_action(cx.listener(Self::vim_open_below))
            .on_action(cx.listener(Self::vim_open_above))
            .on_action(cx.listener(Self::vim_visual))
            .on_action(cx.listener(Self::vim_visual_line))
            .on_action(cx.listener(Self::vim_left))
            .on_action(cx.listener(Self::vim_right))
            .on_action(cx.listener(Self::vim_down))
            .on_action(cx.listener(Self::vim_up))
            .on_action(cx.listener(Self::vim_word_forward))
            .on_action(cx.listener(Self::vim_word_back))
            .on_action(cx.listener(Self::vim_word_end))
            .on_action(cx.listener(Self::vim_line_start))
            .on_action(cx.listener(Self::vim_first_non_blank_action))
            .on_action(cx.listener(Self::vim_line_end))
            .on_action(cx.listener(Self::vim_doc_start))
            .on_action(cx.listener(Self::vim_doc_end))
            .on_action(cx.listener(Self::vim_delete_char))
            .on_action(cx.listener(Self::vim_delete_char_before))
            .on_action(cx.listener(Self::vim_delete_line))
            .on_action(cx.listener(Self::vim_yank_line))
            .on_action(cx.listener(Self::vim_change_line))
            .on_action(cx.listener(Self::vim_delete_word))
            .on_action(cx.listener(Self::vim_change_word))
            .on_action(cx.listener(Self::vim_delete_to_end))
            .on_action(cx.listener(Self::vim_change_to_end))
            .on_action(cx.listener(Self::vim_paste_after))
            .on_action(cx.listener(Self::vim_paste_before))
            .on_action(cx.listener(Self::vim_join_lines))
            .on_action(cx.listener(Self::vim_visual_delete))
            .on_action(cx.listener(Self::vim_visual_yank))
            .on_action(cx.listener(Self::vim_visual_change))
    }

    /// A mouse click leaves Visual mode (the click sets its own selection).
    pub(crate) fn vim_on_click(&mut self) {
        if self.vim.mode.is_visual() {
            self.vim.mode = VimMode::Normal;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_motions() {
        let t = "foo.bar  baz\nqux";
        assert_eq!(next_word_start(t, 0), 3); // foo| .
        assert_eq!(next_word_start(t, 3), 4); // . | bar
        assert_eq!(next_word_start(t, 4), 9); // bar → baz
        assert_eq!(next_word_start(t, 9), 13); // baz → qux (across the line break)
        assert_eq!(next_word_start(t, 13), t.len());
        assert_eq!(prev_word_start(t, 9), 4);
        assert_eq!(prev_word_start(t, 5), 4);
        assert_eq!(prev_word_start(t, 13), 9);
        assert_eq!(prev_word_start(t, 0), 0);
        assert_eq!(word_end(t, 0), 2);
        assert_eq!(word_end(t, 2), 3);
        assert_eq!(word_end(t, 4), 6);
        assert_eq!(word_end(t, 6), 11);
    }
}
