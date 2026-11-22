//! Capture: jot a task, note, or journal entry without leaving what you're doing.

use aster_editor::{EditorEvent, OrgEditor};
use aster_ui::icons::{Icon, icon};
use aster_ui::{ActiveTheme, components, ramp};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseButton, Subscription, Window,
    div, prelude::*, px,
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
        let tabs: Vec<_> = Template::ALL
            .into_iter()
            .map(|t| {
                components::selector_tab(t.label(), t.label(), t == self.template, cx)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.template = t;
                            window.focus(&this.input.focus_handle(cx));
                            cx.notify();
                        }),
                    )
            })
            .collect();
        components::dialog(cx)
            .w(px(560.))
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(icon(Icon::NoteAdd, px(20.), cx.theme().colors.accent).mr_2())
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_size(ramp::SUBTITLE.size)
                            .line_height(ramp::SUBTITLE.line_height)
                            .mr_3()
                            .child("Capture"),
                    )
                    .children(tabs),
            )
            .child(components::text_box(true, cx).child(self.input.clone()))
            .child(components::caption(
                format!(
                    "{} · Enter to save · ↑↓ change type · Esc cancel",
                    self.template.hint()
                ),
                cx,
            ))
    }
}

impl Focusable for CaptureDialog {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}
