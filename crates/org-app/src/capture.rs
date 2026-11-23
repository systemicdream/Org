//! Capture: jot a task, note, or journal entry without leaving what you're doing.

use crate::settings::{CaptureKind, CaptureTemplate, Settings};
use chrono::{Datelike, Duration, NaiveDate};
use gpui::SharedString;
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseButton, Subscription, Window,
    div, prelude::*, px,
};
use org_doc::meta::Planning;
use org_doc::ops::OpResult;
use org_doc::{Document, dates, meta};
use org_editor::calendar::{Calendar, add_months};
use org_editor::{EditorEvent, OrgEditor};
use org_ui::icons::{Icon, icon};
use org_ui::{ActiveTheme, components, ramp};
use std::rc::Rc;

pub enum CaptureEvent {
    Capture(CaptureTemplate, String, Option<CaptureDate>),
    Dismissed,
}

/// A date for a captured task or note: when to do it, or when it's due.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureDate {
    pub kind: Planning,
    pub date: NaiveDate,
}

pub struct CaptureDialog {
    templates: Vec<CaptureTemplate>,
    /// Index into `templates`.
    template: usize,
    input: Entity<OrgEditor>,
    /// The date to add (tasks and notes), and whether it's a scheduled date or a deadline.
    date: Option<NaiveDate>,
    date_kind: Planning,
    /// The month the calendar shows, while it's open.
    calendar: Option<NaiveDate>,
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
                    let date = this.date.filter(|_| t.kind != CaptureKind::Journal);
                    let date = date.map(|date| CaptureDate {
                        kind: this.date_kind,
                        date,
                    });
                    cx.emit(CaptureEvent::Capture(t.clone(), text, date));
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
            date: None,
            date_kind: Planning::Scheduled,
            calendar: None,
            _subscription: sub,
        }
    }
}

/// "Today", "Tomorrow", or "Fri, Nov 27" (with the year when it isn't this year's).
fn friendly(date: NaiveDate) -> String {
    let today = dates::today();
    match (date - today).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        _ if date.year() == today.year() => date.format("%a, %b %-d").to_string(),
        _ => date.format("%a, %b %-d, %Y").to_string(),
    }
}

impl CaptureDialog {
    fn set_date(&mut self, date: Option<NaiveDate>, window: &mut Window, cx: &mut Context<Self>) {
        self.date = date;
        self.calendar = None;
        window.focus(&self.input.focus_handle(cx));
        cx.notify();
    }

    /// Quick dates, or the chosen date with whether it's scheduled or a deadline.
    fn render_date_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = cx.theme().colors;
        let chip = |id: &'static str, label: SharedString, on: bool| {
            div()
                .id(id)
                .px_2()
                .h(px(24.))
                .flex()
                .items_center()
                .gap_1()
                .rounded(org_ui::RADIUS_CONTROL)
                .border_1()
                .border_color(if on { c.accent } else { c.stroke_control })
                .text_size(ramp::CAPTION.size)
                .text_color(if on { c.accent } else { c.text_secondary })
                .cursor_pointer()
                .hover(move |d| d.bg(c.subtle_secondary))
                .child(label)
        };
        let today = dates::today();
        let row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1p5()
            .child(icon(Icon::CalendarToday, px(16.), c.text_secondary).mr_1());
        let picking = self.calendar.is_some();
        match self.date {
            None => row
                .children(
                    [
                        ("capture-today", "Today", today),
                        ("capture-tomorrow", "Tomorrow", today + Duration::days(1)),
                        ("capture-next-week", "Next week", today + Duration::days(7)),
                    ]
                    .map(|(id, label, date)| {
                        chip(id, label.into(), false).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.set_date(Some(date), window, cx)
                            }),
                        )
                    }),
                )
                .child(
                    chip("capture-pick", "Pick a date…".into(), picking).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.calendar = match this.calendar {
                                Some(_) => None,
                                None => Some(today),
                            };
                            window.focus(&this.input.focus_handle(cx));
                            cx.notify();
                        }),
                    ),
                ),
            Some(date) => row
                .children(
                    [
                        ("capture-scheduled", "Scheduled", Planning::Scheduled),
                        ("capture-deadline", "Deadline", Planning::Deadline),
                    ]
                    .map(|(id, label, kind)| {
                        chip(id, label.into(), self.date_kind == kind).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.date_kind = kind;
                                window.focus(&this.input.focus_handle(cx));
                                cx.notify();
                            }),
                        )
                    }),
                )
                .child(
                    chip("capture-date", friendly(date).into(), picking)
                        .text_color(c.text_primary)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.calendar = match this.calendar {
                                    Some(_) => None,
                                    None => Some(date),
                                };
                                window.focus(&this.input.focus_handle(cx));
                                cx.notify();
                            }),
                        ),
                )
                .child(
                    div()
                        .id("capture-clear-date")
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(org_ui::RADIUS_CONTROL)
                        .cursor_pointer()
                        .hover(move |d| d.bg(c.subtle_secondary))
                        .tooltip(|window, cx| {
                            components::Tooltip::text("Remove the date", window, cx)
                        })
                        .child(icon(Icon::Dismiss, px(12.), c.text_secondary))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| this.set_date(None, window, cx)),
                        ),
                ),
        }
    }
}

impl Render for CaptureDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Journal entries are dated by when they're written; tasks and notes can have a date.
        let dated = self
            .templates
            .get(self.template)
            .is_some_and(|t| t.kind != CaptureKind::Journal);
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
            .children(dated.then(|| self.render_date_row(cx)))
            .children(self.calendar.filter(|_| dated).map(|month| {
                let this = cx.entity().downgrade();
                let on_month = {
                    let this = this.clone();
                    move |delta, _: &mut Window, cx: &mut App| {
                        this.update(cx, |d, cx| {
                            d.calendar = d.calendar.map(|m| add_months(m, delta));
                            cx.notify();
                        })
                        .ok();
                    }
                };
                let on_pick = move |date, window: &mut Window, cx: &mut App| {
                    this.update(cx, |d, cx| d.set_date(Some(date), window, cx))
                        .ok();
                };
                div().flex().child(
                    div()
                        .p_2()
                        .border_1()
                        .border_color(cx.theme().colors.stroke_surface)
                        .rounded(org_ui::RADIUS_OVERLAY)
                        .child(
                            Calendar {
                                month,
                                selected: self.date,
                                today: dates::today(),
                                on_month: Rc::new(on_month),
                                on_pick: Rc::new(on_pick),
                                on_clear: None,
                            }
                            .render(cx),
                        ),
                )
            }))
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
            stamp: org_doc::dates::inactive_now(),
            day_title: org_doc::dates::format_date(org_doc::dates::today()),
            time: chrono::Local::now().format("%H:%M").to_string(),
        }
    }
}

/// The edit that files `text` in `doc` (the template's target file) as `template` says.
pub fn capture_op(
    doc: &Document,
    template: &CaptureTemplate,
    text: &str,
    date: Option<CaptureDate>,
    at: &CaptureTime,
) -> Option<OpResult> {
    // The planning line goes right under the heading, before the capture time.
    let stamp = &match date {
        Some(d) => format!(
            "{} {}\n{}",
            d.kind.keyword(),
            dates::active_timestamp(d.date),
            at.stamp
        ),
        None => at.stamp.clone(),
    };
    let CaptureTime {
        day_title, time, ..
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
        run_dated(text, kind, heading, None)
    }

    fn run_dated(
        text: &str,
        kind: CaptureKind,
        heading: &str,
        date: Option<CaptureDate>,
    ) -> String {
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
        let r = capture_op(&doc, &template, "Call Sam", date, &at).unwrap();
        let mut out = text.to_string();
        r.tx.apply(&mut out);
        out
    }

    #[test]
    fn captures_with_a_date() {
        let date = Some(CaptureDate {
            kind: Planning::Deadline,
            date: NaiveDate::from_ymd_opt(2026, 11, 27).unwrap(),
        });
        assert_eq!(
            run_dated("", CaptureKind::Task, "", date),
            "* TODO Call Sam\nDEADLINE: <2026-11-27 Fri>\n[2026-11-22 Sun 10:00]\n"
        );
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
