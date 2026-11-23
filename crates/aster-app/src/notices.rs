//! Notices that stay until dealt with (Fluent InfoBars at the top of the content layer): a new
//! version is available, or Aster crashed last time.

use std::path::PathBuf;

use aster_ui::components::{self, ButtonKind};
use aster_ui::icons::{Icon, icon};
use aster_ui::{ActiveTheme, ramp};
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use crate::release::{self, Update, UpdateStatus};
use crate::settings::Settings;
use crate::workspace::Workspace;

pub enum Notice {
    Update(Update),
    /// The crash report's path.
    Crash(PathBuf),
}

impl Workspace {
    /// At startup: offer to report a crash from last time, and check for updates (if enabled).
    pub(crate) fn startup_notices(&mut self, cx: &mut Context<Self>) {
        if let Some(report) = release::pending_crash() {
            self.notices.push(Notice::Crash(report));
            cx.notify();
        }
        if cx.global::<Settings>().check_for_updates {
            self.check_for_updates(false, cx);
        }
    }

    /// Check for a newer release in the background. A manual check reports its result either
    /// way; the automatic one only speaks up when there's something new.
    pub(crate) fn check_for_updates(&mut self, manual: bool, cx: &mut Context<Self>) {
        let task = cx
            .background_executor()
            .spawn(async move { release::check_for_update(manual) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |ws, cx| match result {
                Ok(UpdateStatus::Available(update)) => {
                    ws.notices.retain(|n| !matches!(n, Notice::Update(_)));
                    ws.notices.push(Notice::Update(update));
                    cx.notify();
                }
                Ok(UpdateStatus::UpToDate) if manual => ws.toast(
                    format!("You have the latest version (Aster {})", release::VERSION),
                    false,
                    cx,
                ),
                Err(err) if manual => {
                    ws.toast(format!("Couldn't check for updates: {err:#}"), true, cx)
                }
                _ => {}
            })
            .ok();
        })
        .detach();
    }

    fn close_notice(&mut self, cx: &mut Context<Self>) {
        if self.notices.is_empty() {
            return;
        }
        match self.notices.remove(0) {
            Notice::Update(update) => release::dismiss_update(&update.version),
            Notice::Crash(report) => release::mark_crash_seen(&report),
        }
        cx.notify();
    }

    pub(crate) fn render_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        // One at a time; a crash report (queued first) comes before an update.
        let notice = self.notices.first()?;
        let c = cx.theme().colors;
        let (glyph, glyph_color, bg, title, message): (_, _, _, SharedString, SharedString) =
            match notice {
                Notice::Update(update) => (
                    Icon::Info,
                    c.accent,
                    c.card,
                    format!("Aster {} is available", update.version).into(),
                    format!("You have {}.", release::VERSION).into(),
                ),
                Notice::Crash(_) => (
                    Icon::Warning,
                    c.caution,
                    c.caution_bg,
                    "Aster quit unexpectedly last time".into(),
                    "Sending the crash report helps fix it. You'll see it before anything \
                     is sent."
                        .into(),
                ),
            };
        let buttons = match notice {
            Notice::Update(update) => {
                let url = update.url.clone();
                div().flex().gap_2().child(
                    components::button("notice-download", "Download", ButtonKind::Accent, cx)
                        .on_click(cx.listener(move |ws, _, _, cx| {
                            cx.open_url(&url);
                            ws.close_notice(cx);
                        })),
                )
            }
            Notice::Crash(report) => {
                let (report_a, report_b) = (report.clone(), report.clone());
                div()
                    .flex()
                    .gap_2()
                    .child(
                        components::button(
                            "notice-report",
                            "Report on GitHub",
                            ButtonKind::Accent,
                            cx,
                        )
                        .on_click(cx.listener(move |ws, _, _, cx| {
                            let text = std::fs::read_to_string(&report_a).unwrap_or_default();
                            cx.open_url(&release::crash_issue_url(&text));
                            ws.close_notice(cx);
                        })),
                    )
                    .child(
                        components::button("notice-show", "Show report", ButtonKind::Standard, cx)
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.reveal_path(&report_b);
                            })),
                    )
            }
        };
        Some(
            div()
                .flex_shrink_0()
                .m_2()
                .px_3()
                .py_2()
                .flex()
                .items_center()
                .gap_3()
                .bg(bg)
                .border_1()
                .border_color(c.stroke_surface)
                .rounded(aster_ui::RADIUS_CONTROL)
                .text_size(ramp::BODY.size)
                .line_height(ramp::BODY.line_height)
                .child(icon(glyph, px(16.), glyph_color))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .gap_x_2()
                        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(title))
                        .child(div().text_color(c.text_secondary).child(message)),
                )
                .child(buttons)
                .child(
                    components::icon_button("notice-close", Icon::Dismiss, cx)
                        .tooltip(|window, cx| components::Tooltip::text("Close", window, cx))
                        .on_click(cx.listener(|ws, _, _, cx| ws.close_notice(cx))),
                )
                .into_any_element(),
        )
    }
}
