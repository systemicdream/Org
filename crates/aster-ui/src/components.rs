//! Fluent building blocks shared by the app (docs/DESIGN.md §7). Each returns a styled element
//! that callers extend with content and handlers.

use gpui::{App, Div, ElementId, FontWeight, Hsla, SharedString, Stateful, div, prelude::*, px};

use crate::{ActiveTheme, Elevation, RADIUS_CONTROL, RADIUS_OVERLAY, ramp};

/// A floating surface for menus, pickers and popovers: flyout fill, hairline outline, 8 px
/// radius, elevation 16, 4 px padding.
pub fn flyout(cx: &App) -> Div {
    let t = cx.theme();
    div()
        .occlude()
        .bg(t.colors.flyout)
        .border_1()
        .border_color(t.colors.stroke_surface)
        .rounded(RADIUS_OVERLAY)
        .shadow(t.shadow(Elevation::Flyout))
        .p_1()
        .text_size(ramp::BODY.size)
        .line_height(ramp::BODY.line_height)
        .text_color(t.colors.text_primary)
}

/// A dialog-level surface (command palette, capture): like a flyout but higher.
pub fn dialog(cx: &App) -> Div {
    flyout(cx).shadow(cx.theme().shadow(Elevation::Dialog))
}

/// A list/navigation row: hover fill, and when selected a subtle fill plus the 3 × 16 px
/// accent indicator at its left edge.
pub fn list_item(id: impl Into<ElementId>, selected: bool, cx: &App) -> Stateful<Div> {
    let t = cx.theme();
    let (hover, sel, accent) = (
        t.colors.subtle_secondary,
        t.colors.subtle_secondary,
        t.colors.accent,
    );
    div()
        .id(id)
        .relative()
        .min_h(px(32.))
        .px_3()
        .rounded(RADIUS_CONTROL)
        .flex()
        .items_center()
        .gap_2()
        .cursor_pointer()
        .when(selected, |d| {
            d.bg(sel).child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(8.))
                    .w(px(3.))
                    .h(px(16.))
                    .rounded(px(1.5))
                    .bg(accent),
            )
        })
        .when(!selected, |d| d.hover(move |s| s.bg(hover)))
}

/// A text box frame: control fill, hairline border, and a bottom line that becomes a 2 px
/// accent line when focused.
pub fn text_box(focused: bool, cx: &App) -> Div {
    let t = cx.theme();
    div()
        .relative()
        .overflow_hidden()
        .px_2()
        .py_1()
        .rounded(RADIUS_CONTROL)
        .border_1()
        .border_color(t.colors.stroke_control)
        .bg(if focused {
            t.colors.control_input_active
        } else {
            t.colors.control
        })
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(px(if focused { 2. } else { 1. }))
                .bg(if focused {
                    t.colors.accent
                } else {
                    t.colors.stroke_control_strong
                }),
        )
}

/// A selector-bar tab: secondary text at rest; selected is primary, semibold, with a 3 × 16 px
/// accent pill centered underneath.
pub fn selector_tab(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let t = cx.theme();
    let hover = t.colors.subtle_secondary;
    div()
        .id(id)
        .relative()
        .h(px(32.))
        .px_3()
        .rounded(RADIUS_CONTROL)
        .flex()
        .items_center()
        .cursor_pointer()
        .text_color(if selected {
            t.colors.text_primary
        } else {
            t.colors.text_secondary
        })
        .when(selected, |d| d.font_weight(FontWeight::SEMIBOLD))
        .hover(move |s| s.bg(hover))
        .child(label.into())
        .when(selected, |d| {
            d.child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w(px(16.))
                            .h(px(3.))
                            .rounded(px(1.5))
                            .bg(t.colors.accent),
                    ),
            )
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// The primary action: accent fill.
    Accent,
    /// Control fill with a hairline border.
    Standard,
    /// Transparent until hovered (toolbars, icon buttons).
    Subtle,
}

/// A 32 px button.
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    cx: &App,
) -> Stateful<Div> {
    let t = cx.theme();
    let c = t.colors;
    let base = div()
        .id(id)
        .h(px(32.))
        .px_3()
        .flex()
        .items_center()
        .justify_center()
        .rounded(RADIUS_CONTROL)
        .cursor_pointer()
        .child(label.into());
    match kind {
        ButtonKind::Accent => base
            .bg(c.accent)
            .text_color(c.text_on_accent)
            .hover(move |s| s.bg(c.accent_secondary))
            .active(move |s| s.bg(c.accent_tertiary)),
        ButtonKind::Standard => base
            .bg(c.control)
            .border_1()
            .border_color(c.stroke_control)
            .text_color(c.text_primary)
            .hover(move |s| s.bg(c.control_secondary))
            .active(move |s| s.bg(c.control_tertiary)),
        ButtonKind::Subtle => base
            .text_color(c.text_primary)
            .hover(move |s| s.bg(c.subtle_secondary))
            .active(move |s| s.bg(c.subtle_tertiary)),
    }
}

/// A small inline pill (TODO keyword, tag, date) with the given colors.
pub fn pill(fg: Hsla, bg: Hsla) -> Div {
    div().px_1().rounded(RADIUS_CONTROL).text_color(fg).bg(bg)
}

/// A caption-style section header (e.g. "Files" in the sidebar).
pub fn section_header(label: impl Into<SharedString>, cx: &App) -> Div {
    let t = cx.theme();
    div()
        .px_3()
        .pt_4()
        .pb_1()
        .text_size(ramp::CAPTION.size)
        .line_height(ramp::CAPTION.line_height)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.text_secondary)
        .child(label.into())
}

/// Caption text in a muted color (hints, shortcuts, metadata).
pub fn caption(text: impl Into<SharedString>, cx: &App) -> Div {
    let t = cx.theme();
    div()
        .text_size(ramp::CAPTION.size)
        .line_height(ramp::CAPTION.line_height)
        .text_color(t.colors.text_tertiary)
        .child(text.into())
}

/// A 32 × 32 subtle button holding a 16 px icon (command bars, toolbars).
pub fn icon_button(id: impl Into<ElementId>, icon: crate::icons::Icon, cx: &App) -> Stateful<Div> {
    let c = cx.theme().colors;
    div()
        .id(id)
        .size(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(RADIUS_CONTROL)
        .cursor_pointer()
        .hover(move |s| s.bg(c.subtle_secondary))
        .active(move |s| s.bg(c.subtle_tertiary))
        .child(crate::icons::icon(icon, px(16.), c.text_secondary))
}

/// A Fluent tooltip: small flyout surface with body text, elevation 8.
pub struct Tooltip {
    text: SharedString,
}

impl Tooltip {
    /// Build a tooltip view for `.tooltip(...)`.
    pub fn text(
        text: impl Into<SharedString>,
        _: &mut gpui::Window,
        cx: &mut App,
    ) -> gpui::AnyView {
        let text = text.into();
        cx.new(|_| Tooltip { text }).into()
    }
}

impl gpui::Render for Tooltip {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        div()
            .px_2()
            .py_1()
            .bg(t.colors.flyout)
            .border_1()
            .border_color(t.colors.stroke_surface)
            .rounded(RADIUS_CONTROL)
            .shadow(t.shadow(Elevation::Tooltip))
            .text_size(ramp::CAPTION.size)
            .line_height(ramp::CAPTION.line_height)
            .text_color(t.colors.text_primary)
            .child(self.text.clone())
    }
}
