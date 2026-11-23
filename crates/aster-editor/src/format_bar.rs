//! The formatting toolbar shown over selected text: bold, italic, underline, strikethrough,
//! code and link.

use aster_doc::emphasis::{self, Emphasis};
use aster_ui::ActiveTheme;
use aster_ui::icons::{Icon, icon};
use gpui::{
    AnyElement, Context, FontWeight, MouseButton, Window, anchored, deferred, div, prelude::*, px,
};

use crate::popup::PopupAnchor;
use crate::{EditorMode, OrgEditor};

/// What a toolbar button shows.
enum Face {
    /// A letter styled like the formatting it applies.
    Letter(&'static str, Emphasis),
    Icon(Icon),
}

impl OrgEditor {
    pub(crate) fn format_selection(&mut self, e: Option<Emphasis>, cx: &mut Context<Self>) {
        let sel = self.selection();
        let result = match e {
            Some(e) => emphasis::toggle(&self.doc, sel, e),
            None => emphasis::link(&self.doc, sel),
        };
        self.apply_op(result, cx);
    }

    /// The toolbar, above the start of a finished selection while the editor has focus.
    pub(crate) fn render_format_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sel = self.selection();
        if self.mode != EditorMode::Full
            || sel.is_empty()
            || self.is_selecting
            || self.popup.is_some()
            || !self.focus_handle.is_focused(window)
        {
            return None;
        }
        let range = sel.range();
        let start = self.bounds_for_offsets(range.start..range.start)?;
        let active = emphasis::active(&self.doc, range);
        let c = cx.theme().colors;
        let buttons = [
            ("format-bold", Face::Letter("B", Emphasis::Bold), "Bold"),
            (
                "format-italic",
                Face::Letter("I", Emphasis::Italic),
                "Italic",
            ),
            (
                "format-underline",
                Face::Letter("U", Emphasis::Underline),
                "Underline",
            ),
            (
                "format-strike",
                Face::Letter("S", Emphasis::Strike),
                "Strikethrough",
            ),
            ("format-code", Face::Icon(Icon::Code), "Code"),
            ("format-link", Face::Icon(Icon::Link), "Link"),
        ]
        .into_iter()
        .map(|(id, face, tip)| {
            let e = match face {
                Face::Letter(_, e) => Some(e),
                Face::Icon(Icon::Code) => Some(Emphasis::Code),
                Face::Icon(_) => None,
            };
            let on = e.is_some_and(|e| active.contains(&e));
            let fg = if on { c.accent } else { c.text_primary };
            let button = div()
                .id(id)
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(aster_ui::RADIUS_CONTROL)
                .cursor_pointer()
                .text_color(fg)
                .when(on, |d| d.bg(c.subtle_secondary))
                .hover(move |d| d.bg(c.subtle_secondary))
                .tooltip(move |window, cx| aster_ui::components::Tooltip::text(tip, window, cx))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        // Keep the click from starting a new selection in the text.
                        cx.stop_propagation();
                        this.format_selection(e, cx);
                    }),
                );
            match face {
                Face::Letter(letter, e) => {
                    let text = div().text_size(px(15.)).child(letter);
                    button.child(match e {
                        Emphasis::Bold => text.font_weight(FontWeight::BOLD),
                        Emphasis::Italic => text.font_weight(FontWeight::MEDIUM).italic(),
                        Emphasis::Underline => text.underline().text_decoration_color(fg),
                        Emphasis::Strike => text.line_through().text_decoration_color(fg),
                        Emphasis::Code => text,
                    })
                }
                Face::Icon(glyph) => button.child(icon(glyph, px(16.), fg)),
            }
        });
        let bar = aster_ui::components::flyout(cx)
            .flex()
            .gap_0p5()
            .children(buttons)
            // Clicks between buttons mustn't reach the text either.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        Some(
            deferred(PopupAnchor::above(start).place(anchored()).child(bar))
                .with_priority(1)
                .into_any_element(),
        )
    }
}
