//! Platform text input (typing, IME composition, emoji palette) for [`OrgEditor`].

use std::ops::Range;

use aster_doc::EditOrigin;
use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};

use crate::OrgEditor;

impl OrgEditor {
    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8 = 0;
        let mut utf16 = 0;
        for ch in self.doc.text().chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.doc.text()[..offset].chars().map(char::len_utf16).sum()
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
        Some(self.doc.text()[range].to_string())
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
        // Vim Normal/Visual mode: keys are commands, never text.
        if self.vim_blocks_text(cx) {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        // Committing or cancelling an IME composition belongs to the composition's undo step.
        let origin = if new_text.is_empty() && self.marked_range.is_none() {
            EditOrigin::Delete
        } else {
            EditOrigin::Insert
        };
        // "/" at the start of a line or after a space opens the slash menu.
        let opens_slash = new_text == "/"
            && self.mode == crate::EditorMode::Full
            && self.marked_range.is_none()
            && self.doc.text()[..range.start]
                .chars()
                .next_back()
                .is_none_or(char::is_whitespace);
        self.edit(range.clone(), new_text, origin, cx);
        if opens_slash {
            self.open_slash(range.start, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.vim_blocks_text(cx) {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.edit(range.clone(), new_text, EditOrigin::Insert, cx);
        self.marked_range =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        // The new selection is relative to the inserted (marked) text, in UTF-16 units.
        let to_utf8 = |u16_off: usize| {
            let mut utf16 = 0;
            let mut utf8 = 0;
            for ch in new_text.chars() {
                if utf16 >= u16_off {
                    break;
                }
                utf16 += ch.len_utf16();
                utf8 += ch.len_utf8();
            }
            range.start + utf8
        };
        self.selected_range = new_selected_range_utf16
            .map(|sel| to_utf8(sel.start)..to_utf8(sel.end))
            .unwrap_or_else(|| {
                let end = range.start + new_text.len();
                end..end
            });
        self.selection_reversed = false;
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        self.bounds_for_offsets(range)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.index_for_position(point, window);
        Some(self.offset_to_utf16(offset))
    }
}
