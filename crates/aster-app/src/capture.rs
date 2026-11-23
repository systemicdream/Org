//! Capture: jot a task, note, or journal entry without leaving what you're doing.

use crate::settings::{CaptureKind, CaptureTemplate, Settings};
use aster_doc::ops::OpResult;
use aster_doc::{Document, meta};
use aster_editor::{EditorEvent, OrgEditor};
use aster_ui::icons::{Icon, icon};
use aster_ui::{ActiveTheme, components, ramp};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseButton, Subscription, Window,
    div, prelude::*, px,
};

pub enum CaptureEvent {
    Capture(CaptureTemplate, String),
    Dismissed,
}

pub struct CaptureDialog {
    templates: Vec<CaptureTemplate>,
    /// Index into `templates`.
    template: usize,
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
                if let Some(t) = this
                    .templates
                    .get(this.template)
                    .filter(|_| !text.is_empty())
                {
                    cx.emit(CaptureEvent::Capture(t.clone(), text));
                }
            }
            EditorEvent::Cancel => cx.emit(CaptureEvent::Dismissed),
            EditorEvent::Navigate { up } => {
                let (i, n) = (this.template, this.templates.len().max(1));
                this.template = if *up { (i + n - 1) % n } else { (i + 1) % n };
                cx.notify();
            }
            _ => {}
        });
        window.focus(&input.focus_handle(cx));
        Self {
            templates: cx.global::<Settings>().capture_templates.clone(),
            template: 0,
            input,
            _subscription: sub,
        }
    }
}

impl Render for CaptureDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs: Vec<_> = (0..self.templates.len())
            .map(|t| {
                let label = self.templates[t].name.clone();
                let label = if label.is_empty() {
                    self.templates[t].kind.label().to_string()
                } else {
                    label
                };
                components::selector_tab(("capture-template", t), label, t == self.template, cx)
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
            // Clicking elsewhere closes it, unless something's been typed (so a stray click
            // doesn't lose it).
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if this.input.read(cx).text().trim().is_empty() {
                    cx.emit(CaptureEvent::Dismissed);
                }
            }))
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
                    .flex_wrap()
                    .children(tabs)
                    .child(div().flex_1())
                    .child(
                        components::icon_button("capture-close", Icon::Dismiss, cx)
                            .size(px(28.))
                            .tooltip(|window, cx| components::Tooltip::text("Cancel", window, cx))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(CaptureEvent::Dismissed))),
                    ),
            )
            .child(components::text_box(true, cx).child(self.input.clone()))
            .child(components::caption(
                format!(
                    "{} · Enter to save · ↑↓ change template · Esc cancel",
                    self.templates
                        .get(self.template)
                        .map(CaptureTemplate::describe)
                        .unwrap_or_default()
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

/// When a capture happens: the inactive timestamp for new entries, today's journal heading
/// title, and the time for journal entries.
pub struct CaptureTime {
    pub stamp: String,
    pub day_title: String,
    pub time: String,
}

impl CaptureTime {
    pub fn now() -> Self {
        Self {
            stamp: aster_doc::dates::inactive_now(),
            day_title: aster_doc::dates::format_date(aster_doc::dates::today()),
            time: chrono::Local::now().format("%H:%M").to_string(),
        }
    }
}

/// The edit that files `text` in `doc` (the template's target file) as `template` says.
pub fn capture_op(
    doc: &Document,
    template: &CaptureTemplate,
    text: &str,
    at: &CaptureTime,
) -> Option<OpResult> {
    let CaptureTime {
        stamp,
        day_title,
        time,
    } = at;
    match template.kind {
        CaptureKind::Journal => {
            let day = doc
                .headings()
                .iter()
                .find(|h| h.level == 1 && h.title == *day_title)
                .map(|h| h.range.start);
            match day {
                Some(d) => meta::insert_subtree(doc, Some(d), &format!("** {time} {text}\n")),
                None => {
                    meta::insert_subtree(doc, None, &format!("* {day_title}\n** {time} {text}\n"))
                }
            }
        }
        CaptureKind::Task | CaptureKind::Note => {
            let keyword = if template.kind == CaptureKind::Task {
                "TODO "
            } else {
                ""
            };
            let heading = template.heading.trim();
            if heading.is_empty() {
                return meta::insert_subtree(doc, None, &format!("* {keyword}{text}\n{stamp}\n"));
            }
            match doc.headings().iter().find(|h| h.title.trim() == heading) {
                Some(h) => {
                    let stars = "*".repeat(h.level + 1);
                    meta::insert_subtree(
                        doc,
                        Some(h.range.start),
                        &format!("{stars} {keyword}{text}\n{stamp}\n"),
                    )
                }
                None => meta::insert_subtree(
                    doc,
                    None,
                    &format!("* {heading}\n** {keyword}{text}\n{stamp}\n"),
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, kind: CaptureKind, heading: &str) -> String {
        let doc = Document::new(text.to_string());
        let template = CaptureTemplate {
            name: "t".into(),
            kind,
            file: "inbox.org".into(),
            heading: heading.into(),
        };
        let at = CaptureTime {
            stamp: "[2026-11-22 Sun 10:00]".into(),
            day_title: "2026-11-22 Sunday".into(),
            time: "10:00".into(),
        };
        let r = capture_op(&doc, &template, "Call Sam", &at).unwrap();
        let mut out = text.to_string();
        r.tx.apply(&mut out);
        out
    }

    #[test]
    fn captures_to_top_level_and_under_headings() {
        assert_eq!(
            run("", CaptureKind::Task, ""),
            "* TODO Call Sam\n[2026-11-22 Sun 10:00]\n"
        );
        assert_eq!(
            run("* Work\n** Old\n* Home\n", CaptureKind::Note, "Work"),
            "* Work\n** Old\n** Call Sam\n[2026-11-22 Sun 10:00]\n* Home\n"
        );
        assert_eq!(
            run("* Home\n", CaptureKind::Task, "Work"),
            "* Home\n* Work\n** TODO Call Sam\n[2026-11-22 Sun 10:00]\n"
        );
        assert_eq!(
            run(
                "* 2026-11-22 Sunday\n** 09:00 Up\n",
                CaptureKind::Journal,
                "ignored"
            ),
            "* 2026-11-22 Sunday\n** 09:00 Up\n** 10:00 Call Sam\n"
        );
    }
}
