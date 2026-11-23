//! A searchable list in an overlay: the command palette, quick switcher, workspace search and
//! tag browser are all pickers with different providers.

use std::ops::Range;
use std::path::PathBuf;

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseButton, SharedString,
    Subscription, Window, div, prelude::*, px,
};
use org_editor::{EditorEvent, OrgEditor};
use org_ui::icons::{Icon, icon};
use org_ui::{ActiveTheme, components};

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
    /// Leading icon.
    pub icon: Option<Icon>,
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
    cx: &App,
) -> gpui::AnyElement {
    let colors = cx.theme().colors;
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
                        .bg(colors.accent_subtle)
                        .text_color(colors.accent)
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .rounded(org_ui::RADIUS_CONTROL)
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
        let colors = cx.theme().colors;
        let has_query = !self.query.read(cx).text().is_empty();
        let rows: Vec<_> = self
            .items
            .iter()
            .take(MAX_ROWS)
            .enumerate()
            .map(|(i, (item, _))| {
                let selected = i == self.selected;
                components::list_item(("picker-item", i), selected, cx)
                    .py_1()
                    .gap_3()
                    .children(item.icon.map(|g| {
                        icon(
                            g,
                            px(16.),
                            if selected {
                                colors.accent
                            } else {
                                colors.text_secondary
                            },
                        )
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(label_with_highlight(&item.label, &item.highlight, cx))
                            .children(item.detail.clone().map(|d| {
                                components::caption(d, cx)
                                    .text_color(colors.text_secondary)
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                            })),
                    )
                    .children(item.hint.clone().map(|h| components::caption(h, cx)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.confirm(i, cx);
                        }),
                    )
            })
            .collect();
        let more = self.items.len().saturating_sub(MAX_ROWS);
        components::dialog(cx)
            .w(px(600.))
            // Clicking anywhere else closes it (as Escape does).
            .on_mouse_down_out(cx.listener(|_, _, _, cx| cx.emit(PickerEvent::Dismissed)))
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                components::text_box(true, cx)
                    .mb_1()
                    .child(self.query.clone()),
            )
            .children(rows)
            .when(self.items.is_empty() && has_query, |d| {
                d.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_color(colors.text_tertiary)
                        .child("No results"),
                )
            })
            .when(more > 0, |d| {
                d.child(
                    components::caption(format!("{more} more — keep typing to narrow down"), cx)
                        .px_3()
                        .py_1(),
                )
            })
    }
}

impl Focusable for Picker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.query.focus_handle(cx)
    }
}
