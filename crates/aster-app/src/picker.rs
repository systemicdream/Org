//! A searchable list in an overlay: the command palette, quick switcher, workspace search and
//! tag browser are all pickers with different providers.

use std::ops::Range;
use std::path::PathBuf;

use aster_editor::{EditorEvent, OrgEditor};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseButton, SharedString,
    Subscription, Window, div, prelude::*, px, rgb, rgba,
};

/// What choosing an item does.
#[derive(Clone, Debug)]
pub enum PickTarget {
    /// Run the command at this index of the workspace's command list.
    Command(usize),
    /// Open a file.
    File(PathBuf),
    /// Open a file and select a range in it.
    Location(PathBuf, Range<usize>),
    /// Refile the pending heading into this file, under the heading at this offset (or at the
    /// top level).
    Refile(PathBuf, Option<usize>),
}

#[derive(Clone, Default)]
pub struct PickerItem {
    pub label: SharedString,
    /// Highlighted part of `label` (e.g. a search match).
    pub highlight: Option<Range<usize>>,
    /// Secondary text under the label (e.g. a file path).
    pub detail: Option<SharedString>,
    /// Right-aligned hint (e.g. a shortcut).
    pub hint: Option<SharedString>,
}

pub type Provider = Box<dyn Fn(&str, &App) -> Vec<(PickerItem, PickTarget)>>;

pub enum PickerEvent {
    Confirm(PickTarget),
    Dismissed,
}

pub struct Picker {
    query: Entity<OrgEditor>,
    provider: Provider,
    items: Vec<(PickerItem, PickTarget)>,
    selected: usize,
    _subscription: Subscription,
}

impl EventEmitter<PickerEvent> for Picker {}

const MAX_ROWS: usize = 14;

impl Picker {
    pub fn new(
        placeholder: &str,
        initial_query: &str,
        provider: Provider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = placeholder.to_string();
        let query = cx.new(|cx| OrgEditor::single_line(placeholder, window, cx));
        if !initial_query.is_empty() {
            query.update(cx, |q, cx| q.set_text(initial_query, cx));
        }
        let sub = cx.subscribe(&query, |this, _, event: &EditorEvent, cx| match event {
            EditorEvent::Edited => {
                this.refresh(cx);
                cx.notify();
            }
            EditorEvent::Navigate { up } => {
                let n = this.items.len().clamp(1, MAX_ROWS);
                this.selected = if *up {
                    (this.selected + n - 1) % n
                } else {
                    (this.selected + 1) % n
                };
                cx.notify();
            }
            EditorEvent::Confirm { .. } => this.confirm(this.selected, cx),
            EditorEvent::Cancel => cx.emit(PickerEvent::Dismissed),
            EditorEvent::OpenLink(_) => {}
        });
        window.focus(&query.focus_handle(cx));
        let mut this = Self {
            query,
            provider,
            items: Vec::new(),
            selected: 0,
            _subscription: sub,
        };
        this.refresh(cx);
        this
    }

    fn refresh(&mut self, cx: &App) {
        let q = self.query.read(cx).text().to_string();
        self.items = (self.provider)(&q, cx);
        self.selected = 0;
    }

    fn confirm(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some((_, target)) = self.items.get(index) {
            cx.emit(PickerEvent::Confirm(target.clone()));
        }
    }
}

fn label_with_highlight(
    label: &SharedString,
    highlight: &Option<Range<usize>>,
) -> gpui::AnyElement {
    match highlight {
        Some(r)
            if r.end <= label.len()
                && label.is_char_boundary(r.start)
                && label.is_char_boundary(r.end) =>
        {
            div()
                .flex()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(label[..r.start].to_string())
                .child(
                    div()
                        .bg(rgba(0xfde04799))
                        .rounded_sm()
                        .child(label[r.clone()].to_string()),
                )
                .child(label[r.end..].to_string())
                .into_any_element()
        }
        _ => div()
            .overflow_hidden()
            .whitespace_nowrap()
            .child(label.clone())
            .into_any_element(),
    }
}

impl Render for Picker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_query = !self.query.read(cx).text().is_empty();
        let rows = self
            .items
            .iter()
            .take(MAX_ROWS)
            .enumerate()
            .map(|(i, (item, _))| {
                let selected = i == self.selected;
                div()
                    .id(("picker-item", i))
                    .mx_1()
                    .px_3()
                    .py_0p5()
                    .rounded_md()
                    .flex()
                    .items_center()
                    .gap_4()
                    .cursor_pointer()
                    .when(selected, |s| s.bg(rgb(0xe0e7ff)))
                    .when(!selected, |s| s.hover(|s| s.bg(rgb(0xf3f4f6))))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(label_with_highlight(&item.label, &item.highlight))
                            .children(item.detail.clone().map(|d| {
                                div()
                                    .text_size(px(12.))
                                    .line_height(px(16.))
                                    .text_color(rgb(0x6b7280))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .child(d)
                            })),
                    )
                    .children(
                        item.hint
                            .clone()
                            .map(|h| div().text_color(rgb(0x6b7280)).child(h)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.confirm(i, cx);
                        }),
                    )
            });
        let more = self.items.len().saturating_sub(MAX_ROWS);
        div()
            .occlude()
            .w(px(600.))
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(rgb(0xe5e7eb))
            .rounded_lg()
            .shadow_lg()
            .text_size(px(14.))
            .line_height(px(24.))
            .text_color(rgb(0x1f2937))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(0xe5e7eb))
                    .child(self.query.clone()),
            )
            .child(
                div()
                    .py_1()
                    .children(rows)
                    .when(self.items.is_empty() && has_query, |d| {
                        d.child(
                            div()
                                .px_4()
                                .py_1()
                                .text_color(rgb(0x9ca3af))
                                .child("No results"),
                        )
                    })
                    .when(more > 0, |d| {
                        d.child(
                            div()
                                .px_4()
                                .text_size(px(12.))
                                .text_color(rgb(0x9ca3af))
                                .child(format!("{more} more — keep typing to narrow down")),
                        )
                    }),
            )
    }
}

impl Focusable for Picker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.query.focus_handle(cx)
    }
}
