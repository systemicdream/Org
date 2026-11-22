//! Capture: jot a task, note, or journal entry without leaving what you're doing.

use aster_editor::{EditorEvent, OrgEditor};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseButton, Subscription, Window,
    div, prelude::*, px, rgb,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    Task,
    Note,
    Journal,
}

impl Template {
    const ALL: [Template; 3] = [Template::Task, Template::Note, Template::Journal];

    fn label(self) -> &'static str {
        match self {
            Template::Task => "Task",
            Template::Note => "Note",
            Template::Journal => "Journal",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Template::Task => "Adds “TODO …” to inbox.org",
            Template::Note => "Adds a note to inbox.org",
            Template::Journal => "Adds a timestamped entry under today in journal.org",
        }
    }
}

pub enum CaptureEvent {
    Capture(Template, String),
    Dismissed,
}

pub struct CaptureDialog {
    template: Template,
    input: Entity<OrgEditor>,
    _subscription: Subscription,
}

impl EventEmitter<CaptureEvent> for CaptureDialog {}

impl CaptureDialog {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| OrgEditor::single_line("What's on your mind?", window, cx));
        let sub = cx.subscribe(&input, |this, input, event: &EditorEvent, cx| match event {
            EditorEvent::Confirm { .. } => {
                let text = input.read(cx).text().trim().to_string();
                if !text.is_empty() {
                    cx.emit(CaptureEvent::Capture(this.template, text));
                }
            }
            EditorEvent::Cancel => cx.emit(CaptureEvent::Dismissed),
            EditorEvent::Navigate { up } => {
                let i = Template::ALL
                    .iter()
                    .position(|t| *t == this.template)
                    .unwrap_or(0);
                let n = Template::ALL.len();
                this.template = Template::ALL[if *up { (i + n - 1) % n } else { (i + 1) % n }];
                cx.notify();
            }
            _ => {}
        });
        window.focus(&input.focus_handle(cx));
        Self {
            template: Template::Task,
            input,
            _subscription: sub,
        }
    }
}

impl Render for CaptureDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = Template::ALL.into_iter().map(|t| {
            let active = t == self.template;
            div()
                .id(t.label())
                .px_3()
                .rounded_md()
                .cursor_pointer()
                .when(active, |d| d.bg(rgb(0xe0e7ff)).text_color(rgb(0x1e3a8a)))
                .when(!active, |d| {
                    d.text_color(rgb(0x4b5563)).hover(|d| d.bg(rgb(0xf3f4f6)))
                })
                .child(t.label())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        this.template = t;
                        window.focus(&this.input.focus_handle(cx));
                        cx.notify();
                    }),
                )
        });
        div()
            .occlude()
            .w(px(560.))
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
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
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .mr_2()
                            .child("Capture"),
                    )
                    .children(tabs),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0xd1d5db))
                    .child(self.input.clone()),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x9ca3af))
                    .child(format!(
                        "{} · Enter to save · ↑↓ change type · Esc cancel",
                        self.template.hint()
                    )),
            )
    }
}

impl Focusable for CaptureDialog {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}
