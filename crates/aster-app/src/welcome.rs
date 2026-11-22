//! First run (no notes folder yet) and the empty state (a folder, but no note open).

use std::path::{Path, PathBuf};

use aster_doc::dates;
use aster_ui::components::{self, ButtonKind};
use aster_ui::icons::{Icon, icon};
use aster_ui::{ActiveTheme, ramp};
use gpui::{AnyElement, Context, Div, Window, div, prelude::*, px};

use crate::file_actions::FileAction;
use crate::workspace::{SAMPLE, Workspace, write_atomic};
use crate::{OpenFolder, QuickOpen, ShowAgenda};

pub const WELCOME_FILE: &str = "Welcome to Aster.org";

/// A starter inbox, with tasks scheduled today so the agenda has something in it.
fn starter_inbox() -> String {
    let today = dates::active_timestamp(dates::today());
    format!(
        "#+TITLE: Inbox\n\n\
         New captures land here. Tick things off when they're done, or move them into other \
         notes with Refile.\n\n\
         * TODO Read the welcome guide\nSCHEDULED: {today}\n\
         * TODO Capture an idea with the + button\nSCHEDULED: {today}\n\
         * TODO Open the agenda to see what's scheduled\n"
    )
}

/// Set up a new notes folder: the welcome guide and a starter inbox (never overwriting).
/// Returns the welcome guide's path.
pub fn create_notes_folder(dir: &Path) -> anyhow::Result<PathBuf> {
    if dir.exists() && !dir.is_dir() {
        anyhow::bail!("{} is a file, not a folder", dir.display());
    }
    std::fs::create_dir_all(dir)?;
    let welcome = dir.join(WELCOME_FILE);
    if !welcome.exists() {
        write_atomic(&welcome, SAMPLE)?;
    }
    let inbox = dir.join("inbox.org");
    if !inbox.exists() {
        write_atomic(&inbox, &starter_inbox())?;
    }
    Ok(welcome)
}

fn feature(glyph: Icon, title: &'static str, text: &'static str, cx: &Context<Workspace>) -> Div {
    let c = cx.theme().colors;
    // A fixed width: text only wraps inside a box of known width.
    div()
        .w(px(208.))
        .p_4()
        .flex()
        .flex_col()
        .gap_2()
        .bg(c.card)
        .border_1()
        .border_color(c.stroke_surface)
        .rounded(aster_ui::RADIUS_OVERLAY)
        .child(icon(glyph, px(24.), c.accent))
        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(title))
        .child(components::caption(text, cx).text_color(c.text_secondary))
}

impl Workspace {
    /// Ask where to put a new notes folder, then set it up and open it.
    pub(crate) fn create_notes_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let start = dirs::document_dir()
            .or_else(std::env::home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let rx = cx.prompt_for_new_path(&start, Some("Notes"));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(dir))) = rx.await else {
                return;
            };
            this.update_in(cx, |ws, window, cx| match create_notes_folder(&dir) {
                Ok(welcome) => {
                    ws.set_root(dir, window, cx);
                    ws.open_path(welcome, window, cx);
                }
                Err(err) => ws.toast(format!("Couldn't create the folder: {err:#}"), true, cx),
            })
            .ok();
        })
        .detach();
    }

    /// Shown when there's no notes folder yet: what Aster is, and how to start.
    pub(crate) fn render_first_run(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = cx.theme().colors;
        div()
            .id("first-run")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .text_size(ramp::BODY.size)
            .line_height(ramp::BODY.line_height)
            .child(
                div()
                    .max_w(px(720.))
                    .px_8()
                    .py(px(64.))
                    .flex()
                    .flex_col()
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_size(ramp::TITLE_LARGE.size)
                                    .line_height(ramp::TITLE_LARGE.line_height)
                                    .child("Welcome to Aster"),
                            )
                            .child(
                                div()
                                    .text_size(ramp::BODY_LARGE.size)
                                    .line_height(ramp::BODY_LARGE.line_height)
                                    .text_color(c.text_secondary)
                                    .child(
                                        "Notes, to-dos and plans in plain Org files, in a folder \
                                         you choose. They're yours: open them in any editor, \
                                         and sync them with any service you like.",
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_3()
                            .child(
                                components::button(
                                    "first-create",
                                    "Create a notes folder…",
                                    ButtonKind::Accent,
                                    cx,
                                )
                                .on_click(cx.listener(
                                    |ws, _, window, cx| ws.create_notes_folder(window, cx),
                                )),
                            )
                            .child(
                                components::button(
                                    "first-open",
                                    "Open an existing folder…",
                                    ButtonKind::Standard,
                                    cx,
                                )
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(OpenFolder), cx)
                                }),
                            )
                            .child(
                                components::button(
                                    "first-sample",
                                    "Look around first",
                                    ButtonKind::Subtle,
                                    cx,
                                )
                                .text_color(c.accent)
                                .on_click(
                                    cx.listener(|ws, _, window, cx| ws.show_sample(window, cx)),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_3()
                            .child(feature(
                                Icon::Document,
                                "Write",
                                "Headings, lists, checkboxes, tables and links, formatted as \
                                 you type.",
                                cx,
                            ))
                            .child(feature(
                                Icon::Agenda,
                                "Plan",
                                "Give tasks dates and deadlines, and see your day and week in \
                                 the agenda.",
                                cx,
                            ))
                            .child(feature(
                                Icon::NoteAdd,
                                "Capture",
                                "Jot down a task or an idea from anywhere in the app, then \
                                 file it later.",
                                cx,
                            )),
                    )
                    .child(
                        components::caption(
                            "A new notes folder starts with a short guide and an inbox. \
                             Already use Emacs Org mode? Open your existing folder; Aster \
                             reads and writes the same files.",
                            cx,
                        )
                        .text_color(c.text_secondary),
                    ),
            )
            .into_any_element()
    }

    /// Shown when a notes folder is open but no note is.
    pub(crate) fn render_empty_state(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = cx.theme().colors;
        let empty_folder = !self.scanning && self.index.read(cx).is_empty();
        let root = self.root.clone();
        let (title, text) = if empty_folder {
            (
                "This folder has no notes yet",
                "Create your first note, or add a short guide to Aster to get started.",
            )
        } else {
            (
                "No note open",
                "Pick a note in the sidebar, or start something new.",
            )
        };
        let mut buttons = div().flex().flex_wrap().justify_center().gap_2().child(
            components::button("empty-new", "New note", ButtonKind::Accent, cx).on_click(
                cx.listener(move |ws, _, window, cx| {
                    if let Some(root) = root.clone() {
                        ws.run_file_action(FileAction::NewNote(root), window, cx);
                    }
                }),
            ),
        );
        if empty_folder {
            buttons = buttons.child(
                components::button(
                    "empty-guide",
                    "Add the welcome guide",
                    ButtonKind::Standard,
                    cx,
                )
                .on_click(cx.listener(|ws, _, window, cx| {
                    let Some(root) = ws.root.clone() else { return };
                    match create_notes_folder(&root) {
                        Ok(welcome) => ws.open_path(welcome, window, cx),
                        Err(err) => ws.toast(format!("{err:#}"), true, cx),
                    }
                })),
            );
        } else {
            buttons = buttons
                .child(
                    components::button("empty-goto", "Go to file…", ButtonKind::Standard, cx)
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(QuickOpen), cx)),
                )
                .child(
                    components::button("empty-agenda", "Agenda", ButtonKind::Standard, cx)
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(ShowAgenda), cx)),
                );
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .p_8()
            .text_size(ramp::BODY.size)
            .line_height(ramp::BODY.line_height)
            .child(icon(Icon::Notebook, px(48.), c.text_tertiary))
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_size(ramp::SUBTITLE.size)
                    .line_height(ramp::SUBTITLE.line_height)
                    .child(title),
            )
            .child(div().text_color(c.text_secondary).child(text))
            .child(buttons.mt_2())
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_folder_gets_guide_and_inbox_without_overwriting() {
        let dir = std::env::temp_dir().join(format!("aster-welcome-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("inbox.org"), "* mine\n").unwrap();
        let welcome = create_notes_folder(&dir.join("Notes")).unwrap();
        assert!(welcome.ends_with(WELCOME_FILE));
        assert!(
            std::fs::read_to_string(dir.join("Notes/inbox.org"))
                .unwrap()
                .contains("SCHEDULED: <")
        );
        // An existing folder keeps its own inbox.
        create_notes_folder(&dir).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("inbox.org")).unwrap(),
            "* mine\n"
        );
        assert!(create_notes_folder(&dir.join("inbox.org")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
