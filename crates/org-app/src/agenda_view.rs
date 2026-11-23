//! The agenda: what's scheduled and due across the notes folder, the TODO list, and Org
//! tag/property searches.

use std::path::PathBuf;

use chrono::{Datelike, Duration, NaiveDate};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, Hsla, KeyBinding, MouseButton,
    SharedString, Subscription, Window, actions, div, prelude::*, px,
};
use org_agenda::{AgendaItem, ItemKind};
use org_doc::dates;
use org_editor::{EditorEvent, OrgEditor};
use org_index::Index;
use org_ui::components::{self, ButtonKind};
use org_ui::icons::Icon;
use org_ui::{ActiveTheme, Colors, ramp};

actions!(
    agenda,
    [
        AgendaUp,
        AgendaDown,
        AgendaOpen,
        AgendaToggleDone,
        AgendaForward,
        AgendaBack,
        AgendaToday,
        AgendaDayView,
        AgendaWeekView,
        AgendaTodoView,
        AgendaSearchView,
        AgendaClose,
    ]
);

const CONTEXT: &str = "Agenda";

/// Org-agenda-like single-key bindings while the agenda has focus.
pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", AgendaUp, c),
        KeyBinding::new("k", AgendaUp, c),
        KeyBinding::new("down", AgendaDown, c),
        KeyBinding::new("j", AgendaDown, c),
        KeyBinding::new("enter", AgendaOpen, c),
        KeyBinding::new("x", AgendaToggleDone, c),
        KeyBinding::new("secondary-enter", AgendaToggleDone, c),
        KeyBinding::new("f", AgendaForward, c),
        KeyBinding::new("right", AgendaForward, c),
        KeyBinding::new("b", AgendaBack, c),
        KeyBinding::new("left", AgendaBack, c),
        KeyBinding::new(".", AgendaToday, c),
        KeyBinding::new("d", AgendaDayView, c),
        KeyBinding::new("w", AgendaWeekView, c),
        KeyBinding::new("t", AgendaTodoView, c),
        KeyBinding::new("s", AgendaSearchView, c),
        KeyBinding::new("escape", AgendaClose, c),
    ]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Day,
    Week,
    Todo,
    Search,
}

pub enum AgendaEvent {
    Open(PathBuf, usize),
    ToggleDone(PathBuf, usize),
    Close,
}

/// A rendered line: a day header or an item.
enum Row {
    Day(NaiveDate, usize),
    Item(AgendaItem),
}

pub struct AgendaView {
    focus_handle: FocusHandle,
    index: Entity<Index>,
    pub mode: Mode,
    start: NaiveDate,
    selected: usize,
    query: Entity<OrgEditor>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<AgendaEvent> for AgendaView {}

impl AgendaView {
    pub fn new(
        index: Entity<Index>,
        mode: Mode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| {
            OrgEditor::single_line(
                "Tags and properties, e.g. +work-home TODO=\"NEXT\"",
                window,
                cx,
            )
        });
        let subs = vec![
            cx.observe(&index, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &query,
                window,
                |this, _, event: &EditorEvent, window, cx| match event {
                    EditorEvent::Edited => {
                        this.selected = 0;
                        cx.notify();
                    }
                    EditorEvent::Confirm { .. } | EditorEvent::Navigate { up: false } => {
                        window.focus(&this.focus_handle);
                    }
                    EditorEvent::Cancel => cx.emit(AgendaEvent::Close),
                    _ => {}
                },
            ),
        ];
        Self {
            focus_handle: cx.focus_handle(),
            index,
            mode,
            start: dates::today(),
            selected: 0,
            query,
            _subscriptions: subs,
        }
    }

    pub fn set_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = mode;
        self.selected = 0;
        let today = dates::today();
        if mode == Mode::Day && today >= self.start && today < self.start + Duration::days(7) {
            // Coming from a week that contains today: show today.
            self.start = today;
        }
        if mode == Mode::Week {
            // Weeks start on Monday, as in Org.
            let back = self.start.weekday().num_days_from_monday() as i64;
            self.start -= Duration::days(back);
        }
        if mode == Mode::Search {
            window.focus(&self.query.focus_handle(cx));
        } else {
            window.focus(&self.focus_handle);
        }
        cx.notify();
    }

    fn rows(&self, cx: &App) -> Vec<Row> {
        let index = self.index.read(cx);
        let today = dates::today();
        match self.mode {
            Mode::Day | Mode::Week => {
                let days = if self.mode == Mode::Day { 1 } else { 7 };
                let mut rows = Vec::new();
                for day in org_agenda::agenda(index, self.start, days, today) {
                    rows.push(Row::Day(day.date, day.items.len()));
                    rows.extend(day.items.into_iter().map(Row::Item));
                }
                rows
            }
            Mode::Todo => org_agenda::todo_list(index)
                .into_iter()
                .map(Row::Item)
                .collect(),
            Mode::Search => {
                let q = self.query.read(cx).text().to_string();
                org_agenda::matches(index, &q)
                    .into_iter()
                    .map(Row::Item)
                    .collect()
            }
        }
    }

    fn items(&self, cx: &App) -> Vec<AgendaItem> {
        self.rows(cx)
            .into_iter()
            .filter_map(|r| match r {
                Row::Item(i) => Some(i),
                Row::Day(..) => None,
            })
            .collect()
    }

    fn selected_item(&self, cx: &App) -> Option<AgendaItem> {
        self.items(cx).into_iter().nth(self.selected)
    }

    // ---- actions ----

    fn up(&mut self, _: &AgendaUp, _: &mut Window, cx: &mut Context<Self>) {
        self.selected = self.selected.saturating_sub(1);
        cx.notify();
    }

    fn down(&mut self, _: &AgendaDown, _: &mut Window, cx: &mut Context<Self>) {
        let n = self.items(cx).len();
        self.selected = (self.selected + 1).min(n.saturating_sub(1));
        cx.notify();
    }

    fn open(&mut self, _: &AgendaOpen, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.selected_item(cx) {
            cx.emit(AgendaEvent::Open(i.path, i.offset));
        }
    }

    fn toggle_done(&mut self, _: &AgendaToggleDone, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.selected_item(cx) {
            cx.emit(AgendaEvent::ToggleDone(i.path, i.offset));
        }
    }

    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let days = match self.mode {
            Mode::Day => 1,
            Mode::Week => 7,
            _ => return,
        };
        self.start += Duration::days(if forward { days } else { -days });
        self.selected = 0;
        cx.notify();
    }

    fn forward(&mut self, _: &AgendaForward, _: &mut Window, cx: &mut Context<Self>) {
        self.step(true, cx);
    }

    fn back(&mut self, _: &AgendaBack, _: &mut Window, cx: &mut Context<Self>) {
        self.step(false, cx);
    }

    fn today(&mut self, _: &AgendaToday, window: &mut Window, cx: &mut Context<Self>) {
        self.start = dates::today();
        let mode = self.mode;
        self.set_mode(mode, window, cx);
    }

    fn close(&mut self, _: &AgendaClose, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(AgendaEvent::Close);
    }

    // ---- rendering ----

    fn tab(&self, label: &'static str, mode: Mode, cx: &mut Context<Self>) -> impl IntoElement {
        components::selector_tab(label, label, self.mode == mode, cx).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, window, cx| this.set_mode(mode, window, cx)),
        )
    }

    fn nav_icon(
        &self,
        id: &'static str,
        glyph: Icon,
        cx: &mut Context<Self>,
        f: fn(&mut Self, &mut Window, &mut Context<Self>),
    ) -> impl IntoElement {
        components::icon_button(id, glyph, cx).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, window, cx| f(this, window, cx)),
        )
    }

    fn nav_button(
        &self,
        id: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
        f: fn(&mut Self, &mut Window, &mut Context<Self>),
    ) -> impl IntoElement {
        components::button(id, label, ButtonKind::Subtle, cx).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, window, cx| f(this, window, cx)),
        )
    }
}

fn kind_label(kind: &ItemKind, c: &Colors) -> (String, Hsla) {
    match kind {
        ItemKind::Scheduled { late: 0 } => ("Scheduled".into(), c.text_secondary),
        ItemKind::Scheduled { late } => (format!("{late} d. late"), c.caution),
        ItemKind::Deadline { days: 0 } => ("Due today".into(), c.critical),
        ItemKind::Deadline { days } if *days < 0 => (format!("{} d. overdue", -days), c.critical),
        ItemKind::Deadline { days } => (format!("Due in {days} d."), c.caution),
        ItemKind::Timestamp | ItemKind::Entry => (String::new(), c.text_secondary),
    }
}

impl Render for AgendaView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let today = dates::today();
        let colors = cx.theme().colors;
        let rows = self.rows(cx);
        let total_items = rows.iter().filter(|r| matches!(r, Row::Item(_))).count();
        self.selected = self.selected.min(total_items.saturating_sub(1));
        let focused = self.focus_handle.is_focused(window);

        let range_label = match self.mode {
            Mode::Day => self.start.format("%A, %-d %B %Y").to_string(),
            Mode::Week => format!("Week of {}", self.start.format("%-d %B %Y")),
            Mode::Todo => format!("{total_items} open to-dos"),
            Mode::Search => format!("{total_items} matching headings"),
        };

        let mut item_i = 0;
        let list = rows
            .into_iter()
            .map(|row| match row {
                Row::Day(date, n) => {
                    let is_today = date == today;
                    div()
                        .pt_3()
                        .pb_1()
                        .px_4()
                        .flex()
                        .gap_2()
                        .items_baseline()
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(if is_today {
                                    colors.accent
                                } else {
                                    colors.text_primary
                                })
                                .child(date.format("%A %-d %B").to_string()),
                        )
                        .child(
                            div()
                                .text_size(ramp::CAPTION.size)
                                .text_color(colors.text_tertiary)
                                .child(if is_today {
                                    "Today".to_string()
                                } else if n == 0 {
                                    "Nothing scheduled".into()
                                } else {
                                    String::new()
                                }),
                        )
                        .into_any_element()
                }
                Row::Item(item) => {
                    let i = item_i;
                    item_i += 1;
                    let selected = i == self.selected;
                    let (label, label_color) = kind_label(&item.kind, &colors);
                    let file = item.rel.trim_end_matches(".org").to_string();
                    let (path, offset) = (item.path.clone(), item.offset);
                    let (path2, offset2) = (item.path.clone(), item.offset);
                    let todo = item.todo.clone().map(|t| {
                        let (fg, bg) = if item.done {
                            (colors.success, colors.success_bg)
                        } else {
                            (colors.critical, colors.critical_bg)
                        };
                        div()
                            .id(("agenda-todo", i))
                            .px_1()
                            .rounded(org_ui::RADIUS_CONTROL)
                            .bg(bg)
                            .text_color(fg)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .child(t)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |_, _, _, cx| {
                                    cx.stop_propagation();
                                    cx.emit(AgendaEvent::ToggleDone(path2.clone(), offset2));
                                }),
                            )
                    });
                    components::list_item(("agenda-item", i), selected && focused, cx)
                        .mx_2()
                        .gap_3()
                        .when(selected && !focused, |d| d.bg(colors.subtle_tertiary))
                        .child(
                            div()
                                .w(px(92.))
                                .flex_shrink_0()
                                .text_size(ramp::CAPTION.size)
                                .text_color(colors.text_secondary)
                                .child(item.time.clone().unwrap_or_default()),
                        )
                        .child(
                            div()
                                .w(px(110.))
                                .flex_shrink_0()
                                .text_size(ramp::CAPTION.size)
                                .text_color(label_color)
                                .child(label),
                        )
                        .children(todo)
                        .children(item.priority.map(|p| {
                            div()
                                .text_color(colors.caution)
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child(format!("[#{p}]"))
                        }))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .when(item.done, |d| {
                                    d.text_color(colors.text_tertiary).line_through()
                                })
                                .child(item.title.clone()),
                        )
                        .child(
                            div()
                                .text_size(ramp::CAPTION.size)
                                .text_color(colors.text_secondary)
                                .child(
                                    item.tags
                                        .iter()
                                        .map(|t| format!(":{t}"))
                                        .collect::<String>()
                                        + if item.tags.is_empty() { "" } else { ":" },
                                ),
                        )
                        .child(
                            div()
                                .w(px(140.))
                                .flex_shrink_0()
                                .text_size(ramp::CAPTION.size)
                                .text_color(colors.text_tertiary)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(file),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.selected = i;
                                cx.emit(AgendaEvent::Open(path.clone(), offset));
                            }),
                        )
                        .into_any_element()
                }
            })
            .collect::<Vec<_>>();

        let empty = total_items == 0 && matches!(self.mode, Mode::Todo | Mode::Search);
        div()
            .id("agenda")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .text_size(ramp::BODY.size)
            .line_height(px(32.))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::toggle_done))
            .on_action(cx.listener(Self::forward))
            .on_action(cx.listener(Self::back))
            .on_action(cx.listener(Self::today))
            .on_action(cx.listener(Self::close))
            .on_action(cx.listener(|this, _: &AgendaDayView, w, cx| this.set_mode(Mode::Day, w, cx)))
            .on_action(cx.listener(|this, _: &AgendaWeekView, w, cx| this.set_mode(Mode::Week, w, cx)))
            .on_action(cx.listener(|this, _: &AgendaTodoView, w, cx| this.set_mode(Mode::Todo, w, cx)))
            .on_action(cx.listener(|this, _: &AgendaSearchView, w, cx| this.set_mode(Mode::Search, w, cx)))
            .child(
                div()
                    .px_4()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(colors.stroke_divider)
                    .child(div().font_weight(gpui::FontWeight::SEMIBOLD).text_size(ramp::SUBTITLE.size).mr_4().child("Agenda"))
                    .child(self.tab("Day", Mode::Day, cx))
                    .child(self.tab("Week", Mode::Week, cx))
                    .child(self.tab("To-do", Mode::Todo, cx))
                    .child(self.tab("Search", Mode::Search, cx))
                    .child(div().flex_1())
                    .when(matches!(self.mode, Mode::Day | Mode::Week), |d| {
                        d.child(self.nav_icon("agenda-back", Icon::ChevronLeft, cx, |t, _, cx| t.step(false, cx)))
                            .child(self.nav_button("agenda-today", "Today", cx, |t, w, cx| t.today(&AgendaToday, w, cx)))
                            .child(self.nav_icon("agenda-forward", Icon::ChevronRight, cx, |t, _, cx| t.step(true, cx)))
                    }),
            )
            .child(
                div()
                    .px_4()
                    .pt_2()
                    .text_color(colors.text_secondary)
                    .text_size(ramp::CAPTION.size)
                    .child(SharedString::from(range_label)),
            )
            .when(self.mode == Mode::Search, |d| {
                d.child(
                    components::text_box(self.query.focus_handle(cx).is_focused(window), cx)
                        .mx_4()
                        .mt_2()
                        .child(self.query.clone()),
                )
            })
            .child(
                div()
                    .id("agenda-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .pb_6()
                    .children(list)
                    .when(empty, |d| {
                        d.child(div().px_4().pt_4().text_color(colors.text_tertiary).child(match self.mode {
                            Mode::Todo => "No open to-dos. Nice.",
                            _ => "No matches. Try a tag like +work, or TODO=\"NEXT\".",
                        }))
                    }),
            )
            .child(
                div()
                    .px_4()
                    .py_1()
                    .border_t_1()
                    .border_color(colors.stroke_divider)
                    .text_size(ramp::CAPTION.size)
                    .text_color(colors.text_tertiary)
                    .child("↑↓ select · Enter open · x done · d/w/t/s views · b/f back/forward · . today · Esc close"),
            )
    }
}

impl Focusable for AgendaView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
