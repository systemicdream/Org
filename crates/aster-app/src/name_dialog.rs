//! A small dialog asking for a name: new note, new folder, rename.

use aster_editor::{EditorEvent, OrgEditor};
use aster_ui::components::{self, ButtonKind};
use aster_ui::{ActiveTheme, ramp};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString, Subscription, Window,
    div, prelude::*, px,
};

pub enum NameEvent {
    Submit(String),
    Dismissed,
}

pub struct NameDialog {
    title: SharedString,
    action: SharedString,
    input: Entity<OrgEditor>,
    error: Option<SharedString>,
    _subscription: Subscription,
}

impl EventEmitter<NameEvent> for NameDialog {}

impl NameDialog {
    pub fn new(
        title: impl Into<SharedString>,
        action: impl Into<SharedString>,
        initial: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            let mut e = OrgEditor::single_line("Name", window, cx);
            e.set_text(initial, cx);
            // Select the name without its extension, as file managers do.
            let stem = initial.strip_suffix(".org").unwrap_or(initial).len();
            e.select_range(0..stem, cx);
            e
        });
        let sub = cx.subscribe(&input, |this, input, event: &EditorEvent, cx| match event {
            EditorEvent::Confirm { .. } => {
                let text = input.read(cx).text().to_string();
                cx.emit(NameEvent::Submit(text));
            }
            EditorEvent::Cancel => cx.emit(NameEvent::Dismissed),
            EditorEvent::Edited => {
                this.error = None;
                cx.notify();
            }
            _ => {}
        });
        window.focus(&input.focus_handle(cx));
        Self {
            title: title.into(),
            action: action.into(),
            input,
            error: None,
            _subscription: sub,
        }
    }

    /// Show why the name didn't work (the dialog stays open).
    pub fn set_error(&mut self, error: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.error = Some(error.into());
        cx.notify();
    }
}

impl Render for NameDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = cx.theme().colors;
        let focused = self.input.focus_handle(cx).is_focused(window);
        components::dialog(cx)
            .w(px(420.))
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_size(ramp::SUBTITLE.size)
                    .line_height(ramp::SUBTITLE.line_height)
                    .child(self.title.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(components::text_box(focused, cx).child(self.input.clone()))
                    .children(
                        self.error
                            .clone()
                            .map(|e| components::caption(e, cx).text_color(c.critical)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        components::button("name-ok", self.action.clone(), ButtonKind::Accent, cx)
                            .min_w(px(96.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let text = this.input.read(cx).text().to_string();
                                cx.emit(NameEvent::Submit(text));
                            })),
                    )
                    .child(
                        components::button("name-cancel", "Cancel", ButtonKind::Standard, cx)
                            .min_w(px(96.))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(NameEvent::Dismissed))),
                    ),
            )
    }
}

impl Focusable for NameDialog {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}
