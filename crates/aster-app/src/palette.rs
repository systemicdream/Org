//! Command palette: a searchable list of every command, with its shortcut.

use aster_editor::{EditorEvent, OrgEditor, format_keystrokes};
use gpui::{
    Action, App, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseButton, Subscription,
    Window, div, prelude::*, px, rgb,
};

pub enum PaletteEvent {
    Run(Box<dyn Action>),
    Dismissed,
}

pub struct CommandPalette {
    query: Entity<OrgEditor>,
    commands: Vec<(&'static str, Box<dyn Action>)>,
    /// Indexes into `commands` matching the query, best first.
    matches: Vec<usize>,
    selected: usize,
    /// Shortcuts are looked up as if this (the document editor) were focused.
    target: FocusHandle,
    _subscription: Subscription,
}

impl EventEmitter<PaletteEvent> for CommandPalette {}

/// Score `name` against `query` as a subsequence match; lower is better. Word-start matches
/// and contiguous runs score best.
fn score(name: &str, query: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }
    let name_l = name.to_lowercase();
    if name_l.starts_with(&query.to_lowercase()) {
        return Some(0);
    }
    if name_l.contains(&query.to_lowercase()) {
        return Some(1);
    }
    let chars: Vec<char> = name_l.chars().collect();
    let mut pos = 0;
    let mut penalty = 10;
    for q in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let found = chars[pos..].iter().position(|&c| c == q)?;
        let at = pos + found;
        let word_start = at == 0 || !chars[at - 1].is_alphanumeric();
        penalty += found + if word_start { 0 } else { 2 };
        pos = at + 1;
    }
    Some(penalty)
}

impl CommandPalette {
    pub fn new(
        commands: Vec<(&'static str, Box<dyn Action>)>,
        target: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| OrgEditor::single_line("Type a command…", window, cx));
        let sub = cx.subscribe(&query, |this, _, event: &EditorEvent, cx| match event {
            EditorEvent::Edited => {
                this.filter(cx);
                cx.notify();
            }
            EditorEvent::Navigate { up } => {
                let n = this.matches.len().max(1);
                this.selected = if *up {
                    (this.selected + n - 1) % n
                } else {
                    (this.selected + 1) % n
                };
                cx.notify();
            }
            EditorEvent::Confirm { .. } => this.run(this.selected, cx),
            EditorEvent::Cancel => cx.emit(PaletteEvent::Dismissed),
            EditorEvent::OpenLink(_) => {}
        });
        window.focus(&query.focus_handle(cx));
        let mut this = Self {
            query,
            commands,
            matches: Vec::new(),
            selected: 0,
            target,
            _subscription: sub,
        };
        this.filter(cx);
        this
    }

    fn filter(&mut self, cx: &App) {
        let q = self.query.read(cx).text().trim().to_string();
        let mut scored: Vec<(usize, usize)> = self
            .commands
            .iter()
            .enumerate()
            .filter_map(|(i, (name, _))| score(name, &q).map(|s| (s, i)))
            .collect();
        scored.sort_by_key(|(s, i)| (*s, *i));
        self.matches = scored.into_iter().map(|(_, i)| i).collect();
        self.selected = 0;
    }

    fn run(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(&i) = self.matches.get(index) {
            cx.emit(PaletteEvent::Run(self.commands[i].1.boxed_clone()));
        }
    }
}

impl Render for CommandPalette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.matches.iter().enumerate().take(12).map(|(row, &i)| {
            let (name, action) = &self.commands[i];
            let keys = window
                .highest_precedence_binding_for_action_in(action.as_ref(), &self.target)
                .map(|b| format_keystrokes(&b))
                .unwrap_or_default();
            let selected = row == self.selected;
            div()
                .id(("palette-item", row))
                .mx_1()
                .px_3()
                .py_0p5()
                .rounded_md()
                .flex()
                .justify_between()
                .gap_4()
                .cursor_pointer()
                .when(selected, |s| s.bg(rgb(0xe0e7ff)))
                .when(!selected, |s| s.hover(|s| s.bg(rgb(0xf3f4f6))))
                .child(*name)
                .child(div().text_color(rgb(0x6b7280)).child(keys))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.run(row, cx);
                    }),
                )
        });
        div()
            .occlude()
            .w(px(520.))
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
                    .when(self.matches.is_empty(), |d| {
                        d.child(
                            div()
                                .px_4()
                                .py_1()
                                .text_color(rgb(0x9ca3af))
                                .child("No matching commands"),
                        )
                    }),
            )
    }
}

impl Focusable for CommandPalette {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.query.focus_handle(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoring() {
        assert_eq!(score("Fold all", ""), Some(0));
        assert!(score("Fold all", "fold") < score("Unfold all", "fold"));
        assert!(score("Move heading up", "mhu").is_some());
        assert!(score("Move heading up", "xyz").is_none());
        assert!(score("New heading below", "nhb") < score("Unfold heading", "nhb").or(Some(999)));
    }
}
