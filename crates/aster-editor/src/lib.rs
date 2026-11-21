//! Phase 0 editor spike: a multi-line Org text editor on raw GPUI primitives.
//!
//! What it proves out:
//! - IME / dead keys / emoji palette through [`EntityInputHandler`]
//! - multi-line layout, cursor, keyboard + mouse selection, clipboard
//! - styling driven by the parsed Org tree (headings, TODO keywords)
//! - scrolling with the cursor kept in view
//!
//! Intentionally missing (Phase 2+): rope buffer, undo, soft wrap, virtualization,
//! word movement, live-preview hiding of markup.

mod spans;

use std::ops::Range;

use aster_org::OrgTree;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, KeyBinding,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ScrollHandle, ShapedLine, SharedString, Style, UTF16Selection, Window, actions, div, fill,
    point, prelude::*, px, relative, rgb, rgba, size,
};
use unicode_segmentation::UnicodeSegmentation;

actions!(
    editor,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        LineStart,
        LineEnd,
        SelectLineStart,
        SelectLineEnd,
        DocStart,
        DocEnd,
        Newline,
        Tab,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
    ]
);

const KEY_CONTEXT: &str = "OrgEditor";

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
        KeyBinding::new("tab", Tab, ctx),
        KeyBinding::new("secondary-a", SelectAll, ctx),
        KeyBinding::new("secondary-v", Paste, ctx),
        KeyBinding::new("secondary-c", Copy, ctx),
        KeyBinding::new("secondary-x", Cut, ctx),
    ];
    if cfg!(target_os = "macos") {
        bindings.extend([
            KeyBinding::new("cmd-left", LineStart, ctx),
            KeyBinding::new("cmd-right", LineEnd, ctx),
            KeyBinding::new("cmd-shift-left", SelectLineStart, ctx),
            KeyBinding::new("cmd-shift-right", SelectLineEnd, ctx),
            KeyBinding::new("cmd-up", DocStart, ctx),
            KeyBinding::new("cmd-down", DocEnd, ctx),
            KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, ctx),
        ]);
    } else {
        bindings.extend([
            KeyBinding::new("ctrl-home", DocStart, ctx),
            KeyBinding::new("ctrl-end", DocEnd, ctx),
        ]);
    }
    cx.bind_keys(bindings);
}

/// Emitted whenever the buffer text changes.
pub struct Edited;

pub struct OrgEditor {
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    text: String,
    tree: OrgTree,
    /// Byte offset of the start of each line. Always starts with 0.
    line_starts: Vec<usize>,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    /// Preferred x position for vertical movement.
    goal_x: Option<Pixels>,
    is_selecting: bool,
    autoscroll: bool,
    layout: Option<Layout>,
}

/// Geometry from the last paint, used for hit-testing and IME bounds.
struct Layout {
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    lines: Vec<ShapedLine>,
}

impl EventEmitter<Edited> for OrgEditor {}

impl OrgEditor {
    pub fn new(text: String, cx: &mut Context<Self>) -> Self {
        let tree = OrgTree::parse(&text);
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            text,
            tree,
            line_starts: Vec::new(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            goal_x: None,
            is_selecting: false,
            autoscroll: false,
            layout: None,
        };
        this.recompute_lines();
        this
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    // ---- buffer -------------------------------------------------------------------------

    fn edit(&mut self, range: Range<usize>, new_text: &str, cx: &mut Context<Self>) {
        self.text.replace_range(range.clone(), new_text);
        self.tree.replace_range(range, new_text);
        debug_assert_eq!(self.tree.text(), self.text);
        self.recompute_lines();
        self.autoscroll = true;
        cx.emit(Edited);
        cx.notify();
    }

    fn recompute_lines(&mut self) {
        self.line_starts.clear();
        self.line_starts.push(0);
        self.line_starts
            .extend(self.text.match_indices('\n').map(|(i, _)| i + 1));
    }

    fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Byte range of line `row`, excluding its line terminator (`\n` or `\r\n`).
    fn line_range(&self, row: usize) -> Range<usize> {
        let start = self.line_starts[row];
        let mut end = self
            .line_starts
            .get(row + 1)
            .map_or(self.text.len(), |next| next - 1);
        if end > start && self.text.as_bytes()[end - 1] == b'\r' && end < self.text.len() {
            end -= 1;
        }
        start..end
    }

    fn row_for_offset(&self, offset: usize) -> usize {
        self.line_starts.partition_point(|&s| s <= offset) - 1
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        if offset == 0 {
            return 0;
        }
        // Treat "\r\n" as one step.
        if self.text[..offset].ends_with("\r\n") {
            return offset - 2;
        }
        self.text[..offset]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.text[offset..]
            .graphemes(true)
            .next()
            .map_or(self.text.len(), |g| offset + g.len())
    }

    // ---- selection ----------------------------------------------------------------------

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.autoscroll = true;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.autoscroll = true;
        cx.notify();
    }

    /// Offset one line up/down from the cursor, keeping the preferred x position.
    fn vertical_target(&mut self, down: bool) -> usize {
        let cursor = self.cursor_offset();
        let row = self.row_for_offset(cursor);
        let Some(layout) = self.layout.as_ref() else {
            return cursor;
        };
        let goal_x = *self
            .goal_x
            .get_or_insert_with(|| layout.lines[row].x_for_index(cursor - self.line_starts[row]));
        if down && row + 1 >= self.line_count() {
            return self.text.len();
        }
        if !down && row == 0 {
            return 0;
        }
        let target = if down { row + 1 } else { row - 1 };
        let col = layout.lines[target].closest_index_for_x(goal_x);
        self.line_starts[target] + col
    }

    fn index_for_position(&self, position: Point<Pixels>) -> usize {
        let Some(layout) = self.layout.as_ref() else {
            return 0;
        };
        if position.y < layout.bounds.top() {
            return 0;
        }
        let row = ((position.y - layout.bounds.top()) / layout.line_height).floor() as usize;
        if row >= self.line_count() {
            return self.text.len();
        }
        let col = layout.lines[row].closest_index_for_x(position.x - layout.bounds.left());
        self.line_starts[row] + col
    }

    // ---- actions ------------------------------------------------------------------------

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        let target = self.vertical_target(false);
        self.move_to(target, cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        let target = self.vertical_target(true);
        self.move_to(target, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        let target = self.vertical_target(false);
        self.select_to(target, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        let target = self.vertical_target(true);
        self.select_to(target, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.move_to(0, cx);
        self.select_to(self.text.len(), cx);
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
        self.move_to(self.text.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        // Preserve the file's line-ending style.
        let eol = if self.text.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        self.replace_text_in_range(None, eol, window, cx);
    }

    fn tab(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, "  ", window, cx);
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.text[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.text[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        self.is_selecting = true;
        self.goal_x = None;
        let offset = self.index_for_position(event.position);
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
        self.autoscroll = false;
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_position(event.position), cx);
        }
    }

    // ---- UTF-16 conversion (platform IME APIs speak UTF-16) -----------------------------

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8 = 0;
        let mut utf16 = 0;
        for ch in self.text.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.text[..offset].chars().map(char::len_utf16).sum()
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }
}

impl EntityInputHandler for OrgEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.edit(range.clone(), new_text, cx);
        let cursor = range.start + new_text.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.goal_x = None;
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.edit(range.clone(), new_text, cx);
        self.marked_range =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        // The new selection is relative to the inserted (marked) text, in UTF-16 units.
        self.selected_range = new_selected_range_utf16
            .map(|sel| {
                let marked = &new_text;
                let to_utf8 = |u16_off: usize| {
                    let mut utf16 = 0;
                    let mut utf8 = 0;
                    for ch in marked.chars() {
                        if utf16 >= u16_off {
                            break;
                        }
                        utf16 += ch.len_utf16();
                        utf8 += ch.len_utf8();
                    }
                    range.start + utf8
                };
                to_utf8(sel.start)..to_utf8(sel.end)
            })
            .unwrap_or_else(|| {
                let end = range.start + new_text.len();
                end..end
            });
        self.selection_reversed = false;
        self.goal_x = None;
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let row = self.row_for_offset(range.start);
        let line = &layout.lines[row];
        let start = self.line_starts[row];
        let end_col = (range.end.min(self.line_range(row).end)).saturating_sub(start);
        let top = layout.bounds.top() + layout.line_height * row as f32;
        Some(Bounds::from_corners(
            point(
                layout.bounds.left() + line.x_for_index(range.start - start),
                top,
            ),
            point(
                layout.bounds.left() + line.x_for_index(end_col),
                top + layout.line_height,
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.index_for_position(point);
        Some(self.offset_to_utf16(offset))
    }
}

// ---- rendering ------------------------------------------------------------------------------

struct EditorElement {
    editor: Entity<OrgEditor>,
}

struct PrepaintState {
    lines: Vec<ShapedLine>,
    line_height: Pixels,
    selections: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let lines = self.editor.read(cx).line_count();
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (window.line_height() * lines as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> PrepaintState {
        let editor = self.editor.read(cx);
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let base = gpui::TextRun {
            len: 0,
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let styled = spans::style_spans(&editor.tree, &editor.text, editor.marked_range.clone());

        let mut lines = Vec::with_capacity(editor.line_count());
        for row in 0..editor.line_count() {
            let range = editor.line_range(row);
            let runs = spans::runs_for_range(&base, &styled, range.clone());
            let text: SharedString = editor.text[range].to_string().into();
            lines.push(
                window
                    .text_system()
                    .shape_line(text, font_size, &runs, None),
            );
        }

        let selection_color = rgba(0x3b82f640);
        let mut selections = Vec::new();
        let sel = editor.selected_range.clone();
        if !sel.is_empty() {
            let (first, last) = (
                editor.row_for_offset(sel.start),
                editor.row_for_offset(sel.end),
            );
            for (row, line) in lines.iter().enumerate().take(last + 1).skip(first) {
                let range = editor.line_range(row);
                let start = sel.start.max(range.start) - range.start;
                let end = sel.end.min(range.end).max(range.start) - range.start;
                let x0 = line.x_for_index(start);
                // Extend the highlight past the line end when the newline is selected.
                let x1 = line.x_for_index(end) + if row < last { px(6.) } else { px(0.) };
                let top = bounds.top() + line_height * row as f32;
                selections.push(fill(
                    Bounds::from_corners(
                        point(bounds.left() + x0, top),
                        point(bounds.left() + x1, top + line_height),
                    ),
                    selection_color,
                ));
            }
        }

        let cursor_offset = editor.cursor_offset();
        let row = editor.row_for_offset(cursor_offset);
        let col = cursor_offset.min(editor.line_range(row).end) - editor.line_starts[row];
        let cursor_top = bounds.top() + line_height * row as f32;
        let cursor = sel.is_empty().then(|| {
            fill(
                Bounds::new(
                    point(bounds.left() + lines[row].x_for_index(col), cursor_top),
                    size(px(2.), line_height),
                ),
                rgb(0x3b82f6),
            )
        });

        // Keep the cursor visible after keyboard navigation or edits.
        if editor.autoscroll {
            let viewport = editor.scroll_handle.bounds();
            let mut offset = editor.scroll_handle.offset();
            let margin = line_height;
            if cursor_top - margin < viewport.top() {
                offset.y += viewport.top() - (cursor_top - margin);
            } else if cursor_top + line_height + margin > viewport.bottom() {
                offset.y -= cursor_top + line_height + margin - viewport.bottom();
            }
            offset.y = offset.y.min(px(0.));
            if offset != editor.scroll_handle.offset() {
                editor.scroll_handle.set_offset(offset);
                window.refresh();
            }
        }

        PrepaintState {
            lines,
            line_height,
            selections,
            cursor,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        for quad in prepaint.selections.drain(..) {
            window.paint_quad(quad);
        }
        let line_height = prepaint.line_height;
        for (row, line) in prepaint.lines.iter().enumerate() {
            let origin = point(bounds.left(), bounds.top() + line_height * row as f32);
            line.paint(origin, line_height, window, cx).ok();
        }
        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        let lines = std::mem::take(&mut prepaint.lines);
        self.editor.update(cx, |editor, _| {
            editor.autoscroll = false;
            editor.layout = Some(Layout {
                bounds,
                line_height,
                lines,
            });
        });
    }
}

impl Render for OrgEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("org-editor")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .track_scroll(&self.scroll_handle)
            .overflow_y_scroll()
            .size_full()
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
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
            .on_action(cx.listener(Self::tab))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .px_8()
            .py_6()
            .child(EditorElement {
                editor: cx.entity(),
            })
    }
}

impl Focusable for OrgEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
