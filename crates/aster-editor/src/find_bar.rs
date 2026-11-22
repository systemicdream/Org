//! Find / replace bar that drives an [`OrgEditor`]'s search.

use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    SharedString, Subscription, Window, div, prelude::*, px, rgb,
};

use crate::{EditorEvent, OrgEditor};

pub enum FindBarEvent {
    /// Escape or the close button: the host should hide the bar and refocus the editor.
    Dismissed,
}

pub struct FindBar {
    target: Entity<OrgEditor>,
    query: Entity<OrgEditor>,
    replacement: Entity<OrgEditor>,
    show_replace: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<FindBarEvent> for FindBar {}

impl FindBar {
    pub fn new(target: Entity<OrgEditor>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| OrgEditor::single_line("Find", window, cx));
        let replacement = cx.new(|cx| OrgEditor::single_line("Replace with", window, cx));
        let subscriptions = vec![
            cx.subscribe(&query, |this, query, event: &EditorEvent, cx| match event {
                EditorEvent::Edited => {
                    let q = query.read(cx).text().to_string();
                    this.target.update(cx, |t, cx| t.set_search_query(&q, cx));
                    cx.notify();
                }
                EditorEvent::Confirm { back } => this.next(*back, cx),
                EditorEvent::Cancel => cx.emit(FindBarEvent::Dismissed),
                EditorEvent::OpenLink(_) => {}
            }),
            cx.subscribe(
                &replacement,
                |this, _, event: &EditorEvent, cx| match event {
                    EditorEvent::Confirm { .. } => this.replace_one(cx),
                    EditorEvent::Cancel => cx.emit(FindBarEvent::Dismissed),
                    EditorEvent::Edited | EditorEvent::OpenLink(_) => {}
                },
            ),
            // Keep the match counter current as the document changes.
            cx.observe(&target, |_, _, cx| cx.notify()),
        ];
        Self {
            target,
            query,
            replacement,
            show_replace: false,
            _subscriptions: subscriptions,
        }
    }

    /// Show the bar (optionally with replace), seeded with `seed` (e.g. the selected text).
    pub fn open(
        &mut self,
        with_replace: bool,
        seed: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_replace |= with_replace;
        if let Some(seed) = seed.filter(|s| !s.is_empty() && !s.contains('\n')) {
            self.query.update(cx, |q, cx| q.set_text(&seed, cx));
        }
        let q = self.query.read(cx).text().to_string();
        self.target.update(cx, |t, cx| t.set_search_query(&q, cx));
        self.query.update(cx, |q, cx| q.select_all_text(cx));
        window.focus(&self.query.focus_handle(cx));
        cx.notify();
    }

    pub fn next(&mut self, back: bool, cx: &mut Context<Self>) {
        self.target.update(cx, |t, cx| t.search_next(back, cx));
        cx.notify();
    }

    fn replace_one(&mut self, cx: &mut Context<Self>) {
        let r = self.replacement.read(cx).text().to_string();
        self.target.update(cx, |t, cx| t.replace_active(&r, cx));
        cx.notify();
    }

    fn replace_all(&mut self, cx: &mut Context<Self>) {
        let r = self.replacement.read(cx).text().to_string();
        self.target.update(cx, |t, cx| t.replace_all(&r, cx));
        cx.notify();
    }
}

fn button(
    id: &'static str,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .px_2()
        .py_0p5()
        .rounded_md()
        .text_color(rgb(0x374151))
        .hover(|s| s.bg(rgb(0xe5e7eb)).cursor_pointer())
        .child(label.into())
        .on_click(on_click)
}

fn input_box(editor: Entity<OrgEditor>) -> impl IntoElement {
    div()
        .w(px(260.))
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(rgb(0xd1d5db))
        .bg(rgb(0xffffff))
        .child(editor)
}

impl Render for FindBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (active, total) = self.target.read(cx).search_status();
        let has_query = !self.query.read(cx).text().is_empty();
        let status = match (has_query, active) {
            (false, _) => String::new(),
            (true, _) if total == 0 => "No results".into(),
            (true, Some(i)) => format!("{i} of {total}"),
            (true, None) => format!("{total} found"),
        };
        let this = cx.entity();

        let find_row = div()
            .flex()
            .items_center()
            .gap_2()
            .child(input_box(self.query.clone()))
            .child(div().min_w(px(80.)).text_color(rgb(0x6b7280)).child(status))
            .child(button("find-prev", "↑", {
                let this = this.clone();
                move |_, _, cx| this.update(cx, |b, cx| b.next(true, cx))
            }))
            .child(button("find-next", "↓", {
                let this = this.clone();
                move |_, _, cx| this.update(cx, |b, cx| b.next(false, cx))
            }))
            .child(button(
                "find-toggle-replace",
                if self.show_replace {
                    "Hide replace"
                } else {
                    "Replace…"
                },
                {
                    let this = this.clone();
                    move |_, _, cx| {
                        this.update(cx, |b, cx| {
                            b.show_replace = !b.show_replace;
                            cx.notify();
                        })
                    }
                },
            ))
            .child(div().flex_1())
            .child(button("find-close", "✕", {
                let this = this.clone();
                move |_, _, cx| this.update(cx, |_, cx| cx.emit(FindBarEvent::Dismissed))
            }));

        let replace_row = self.show_replace.then(|| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(input_box(self.replacement.clone()))
                .child(button("replace-one", "Replace", {
                    let this = this.clone();
                    move |_, _, cx| this.update(cx, |b, cx| b.replace_one(cx))
                }))
                .child(button("replace-all", "Replace all", {
                    let this = this.clone();
                    move |_, _, cx| this.update(cx, |b, cx| b.replace_all(cx))
                }))
        });

        div()
            .flex()
            .flex_col()
            .gap_1()
            .px_4()
            .py_2()
            .bg(rgb(0xf3f4f6))
            .border_b_1()
            .border_color(rgb(0xe5e7eb))
            .text_size(px(14.))
            .line_height(px(20.))
            .child(find_row)
            .children(replace_row)
    }
}

impl Focusable for FindBar {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.query.focus_handle(cx)
    }
}
