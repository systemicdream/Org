//! Popups attached to the editor: menus (TODO state, priority, tags, heading actions),
//! a date picker, a tag input, and the `/` slash menu.

use std::ops::Range;

use aster_doc::meta::{self, Planning};
use aster_doc::{EditOrigin, Selection, Transaction, dates, ops};
use chrono::{Datelike, Duration, NaiveDate};
use gpui::{
    AnyElement, Bounds, Context, Entity, Focusable, MouseButton, Pixels, SharedString,
    Subscription, Window, anchored, deferred, div, prelude::*, px,
};

use crate::{EditorEvent, OrgEditor};
use aster_ui::ActiveTheme;
use aster_ui::icons::{Icon, icon};

/// What a date picker changes.
#[derive(Clone, Debug)]
pub(crate) enum DateTarget {
    Planning(usize, Planning),
    Timestamp(Range<usize>),
    /// Insert a new timestamp at an offset (slash menu "Date").
    Insert(usize),
}

/// Heading-level actions offered by the heading menu.
#[derive(Clone, Copy, Debug)]
pub(crate) enum HeadingOp {
    MoveUp,
    MoveDown,
    Indent,
    Outdent,
    NewBelow,
    ToggleFold,
    Refile,
    Archive,
}

#[derive(Clone, Debug)]
pub(crate) enum Command {
    SetTodo(usize, Option<String>),
    SetPriority(usize, Option<char>),
    RemoveTag(usize, String),
    AddTag(usize),
    AddExistingTag(usize, String),
    TodoMenu(usize),
    PriorityMenu(usize),
    TagsMenu(usize),
    PickDate(DateTarget),
    Heading(usize, HeadingOp),
}

#[derive(Clone)]
pub(crate) struct MenuItem {
    label: SharedString,
    hint: Option<SharedString>,
    checked: bool,
    icon: Option<Icon>,
    command: Command,
}

impl MenuItem {
    fn new(label: impl Into<SharedString>, command: Command) -> Self {
        Self {
            label: label.into(),
            hint: None,
            checked: false,
            icon: None,
            command,
        }
    }

    fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

pub(crate) enum Popup {
    Menu {
        anchor: PopupAnchor,
        title: Option<SharedString>,
        items: Vec<MenuItem>,
        selected: usize,
    },
    Date {
        anchor: PopupAnchor,
        target: DateTarget,
        month: NaiveDate,
        selected: NaiveDate,
        clearable: bool,
    },
    TagInput {
        anchor: PopupAnchor,
        heading: usize,
        input: Entity<OrgEditor>,
        _subscription: Subscription,
    },
    Slash {
        /// Offset of the `/` that opened the menu.
        start: usize,
        selected: usize,
    },
}

/// Navigation keys forwarded from the editor while a popup is open.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavKey {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
}

/// Where a popup attaches to the text: a rectangle (usually a line of text) it opens next to.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct PopupAnchor {
    pub bounds: Bounds<Pixels>,
    /// Prefer opening above the rectangle (the formatting toolbar) rather than below it.
    pub above: bool,
}

/// Space between a popup and the text it belongs to.
const POPUP_GAP: f32 = 4.;

impl PopupAnchor {
    pub(crate) fn below(bounds: Bounds<Pixels>) -> Self {
        Self {
            bounds,
            above: false,
        }
    }

    pub(crate) fn above(bounds: Bounds<Pixels>) -> Self {
        Self {
            bounds,
            above: true,
        }
    }

    /// Position `a` on the preferred side, flipping to the other side of the text (never over
    /// it) when the window has no room.
    pub(crate) fn place(self, a: gpui::Anchored) -> gpui::Anchored {
        let gap = px(POPUP_GAP);
        let span = self.bounds.size.height + gap * 2.;
        // GPUI flips around the anchor point, so anchor on the far side of the text and
        // offset across it: the flipped position then lands just beyond the near side.
        if self.above {
            a.anchor(gpui::Corner::BottomLeft)
                .position(self.bounds.bottom_left() + gpui::point(px(0.), gap))
                .offset(gpui::point(px(0.), -span))
        } else {
            a.position(self.bounds.origin - gpui::point(px(0.), gap))
                .offset(gpui::point(px(0.), span))
        }
    }
}

// ---- slash menu items -------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SlashKind {
    Heading,
    Subheading,
    Todo,
    Checklist,
    Bullets,
    Numbered,
    Table,
    Code,
    Quote,
    Date,
    Link,
    Divider,
    Schedule,
    Deadline,
}

const SLASH_ITEMS: &[(SlashKind, &str, &str)] = &[
    (SlashKind::Heading, "Heading", "title section h1"),
    (SlashKind::Subheading, "Subheading", "child section h2"),
    (SlashKind::Todo, "To-do heading", "task todo"),
    (SlashKind::Checklist, "Checklist", "checkbox task item"),
    (SlashKind::Bullets, "Bulleted list", "list bullet ul"),
    (SlashKind::Numbered, "Numbered list", "list number ol"),
    (SlashKind::Table, "Table", "grid"),
    (SlashKind::Code, "Code block", "source src snippet"),
    (SlashKind::Quote, "Quote", "blockquote citation"),
    (SlashKind::Date, "Date", "today timestamp"),
    (SlashKind::Link, "Link", "url"),
    (SlashKind::Divider, "Divider", "rule line hr"),
    (SlashKind::Schedule, "Schedule…", "scheduled date when"),
    (SlashKind::Deadline, "Deadline…", "due date"),
];

fn slash_icon(kind: SlashKind) -> Icon {
    match kind {
        SlashKind::Heading => Icon::Heading1,
        SlashKind::Subheading => Icon::Heading2,
        SlashKind::Todo | SlashKind::Checklist => Icon::Checkbox,
        SlashKind::Bullets => Icon::BulletList,
        SlashKind::Numbered => Icon::NumberList,
        SlashKind::Table => Icon::Table,
        SlashKind::Code => Icon::Code,
        SlashKind::Quote => Icon::Quote,
        SlashKind::Date => Icon::CalendarToday,
        SlashKind::Link => Icon::Link,
        SlashKind::Divider => Icon::Divider,
        SlashKind::Schedule => Icon::Schedule,
        SlashKind::Deadline => Icon::Deadline,
    }
}

fn slash_matches(query: &str) -> Vec<(SlashKind, &'static str)> {
    let q = query.to_lowercase();
    let word_prefix = |s: &str| {
        s.split([' ', '-'])
            .any(|w| w.to_lowercase().starts_with(&q))
    };
    // Rank: name starts with the query, a word in the name does, a keyword does, the name
    // contains it.
    let mut scored: Vec<(usize, SlashKind, &'static str)> = SLASH_ITEMS
        .iter()
        .filter_map(|(kind, name, keys)| {
            let lname = name.to_lowercase();
            let score = if q.is_empty() || lname.starts_with(&q) {
                0
            } else if word_prefix(name) {
                1
            } else if word_prefix(keys) {
                2
            } else if lname.contains(&q) {
                3
            } else {
                return None;
            };
            Some((score, *kind, *name))
        })
        .collect();
    scored.sort_by_key(|(score, ..)| *score);
    scored.into_iter().map(|(_, k, n)| (k, n)).collect()
}

fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).expect("day 1 exists")
}

fn add_months(d: NaiveDate, months: i32) -> NaiveDate {
    let total = d.year() * 12 + d.month0() as i32 + months;
    NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1)
        .expect("valid month")
}

impl OrgEditor {
    // ---- opening ------------------------------------------------------------------------

    /// Where a popup for a text range goes: under its line, or above when there's no room.
    pub(crate) fn anchor_below(&self, range: Range<usize>) -> PopupAnchor {
        self.bounds_for_offsets(range)
            .map(PopupAnchor::below)
            .unwrap_or_default()
    }

    fn open_menu(
        &mut self,
        anchor: PopupAnchor,
        title: Option<&str>,
        items: Vec<MenuItem>,
        cx: &mut Context<Self>,
    ) {
        let selected = items.iter().position(|i| i.checked).unwrap_or(0);
        self.popup = Some(Popup::Menu {
            anchor,
            title: title.map(|t| SharedString::from(t.to_string())),
            items,
            selected,
        });
        cx.notify();
    }

    pub(crate) fn open_todo_menu(&mut self, h: usize, anchor: PopupAnchor, cx: &mut Context<Self>) {
        let current = meta::heading(&self.doc, h).and_then(|h| h.todo.clone());
        let mut items: Vec<MenuItem> = meta::todo_states(&self.doc)
            .into_iter()
            .map(|(s, done)| {
                let hint = if done { "done" } else { "to do" };
                MenuItem::new(s.clone(), Command::SetTodo(h, Some(s.clone())))
                    .hint(hint)
                    .checked(current.as_deref() == Some(s.as_str()))
            })
            .collect();
        items.push(MenuItem::new("No state", Command::SetTodo(h, None)).checked(current.is_none()));
        self.open_menu(anchor, Some("State"), items, cx);
    }

    pub(crate) fn open_priority_menu(
        &mut self,
        h: usize,
        anchor: PopupAnchor,
        cx: &mut Context<Self>,
    ) {
        let current = meta::heading(&self.doc, h)
            .and_then(|h| h.priority.clone())
            .and_then(|p| p.chars().next());
        let mut items: Vec<MenuItem> = [('A', "highest"), ('B', "medium"), ('C', "lowest")]
            .into_iter()
            .map(|(p, hint)| {
                MenuItem::new(format!("[#{p}]"), Command::SetPriority(h, Some(p)))
                    .hint(hint)
                    .checked(current == Some(p))
            })
            .collect();
        items.push(
            MenuItem::new("No priority", Command::SetPriority(h, None)).checked(current.is_none()),
        );
        self.open_menu(anchor, Some("Priority"), items, cx);
    }

    pub(crate) fn open_tags_menu(&mut self, h: usize, anchor: PopupAnchor, cx: &mut Context<Self>) {
        let tags = meta::heading(&self.doc, h)
            .map(|h| h.tags.clone())
            .unwrap_or_default();
        let mut items: Vec<MenuItem> = tags
            .iter()
            .map(|t| {
                MenuItem::new(format!("Remove :{t}:"), Command::RemoveTag(h, t.clone()))
                    .icon(Icon::Dismiss)
            })
            .collect();
        // Suggest tags used elsewhere in the notes folder.
        items.extend(
            self.known_tags
                .iter()
                .filter(|t| !tags.contains(t))
                .take(8)
                .map(|t| {
                    MenuItem::new(format!("Add :{t}:"), Command::AddExistingTag(h, t.clone()))
                }),
        );
        items.push(MenuItem::new("New tag…", Command::AddTag(h)).icon(Icon::Add));
        self.open_menu(anchor, Some("Tags"), items, cx);
    }

    pub(crate) fn open_heading_menu(
        &mut self,
        h: usize,
        anchor: PopupAnchor,
        cx: &mut Context<Self>,
    ) {
        let key = |s: &str| SharedString::from(s.to_string());
        let mod_key = if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl+"
        };
        let alt = if cfg!(target_os = "macos") {
            "⌥"
        } else {
            "Alt+"
        };
        let items = vec![
            MenuItem::new("Set state…", Command::TodoMenu(h))
                .icon(Icon::Checkbox)
                .hint(key(&format!("{mod_key}↵"))),
            MenuItem::new("Set priority…", Command::PriorityMenu(h)).icon(Icon::Flag),
            MenuItem::new("Tags…", Command::TagsMenu(h)).icon(Icon::Tag),
            MenuItem::new(
                "Schedule…",
                Command::PickDate(DateTarget::Planning(h, Planning::Scheduled)),
            )
            .icon(Icon::Schedule),
            MenuItem::new(
                "Set deadline…",
                Command::PickDate(DateTarget::Planning(h, Planning::Deadline)),
            )
            .icon(Icon::Deadline),
            MenuItem::new(
                "New heading below",
                Command::Heading(h, HeadingOp::NewBelow),
            )
            .icon(Icon::Add)
            .hint(key(&format!("{alt}↵"))),
            MenuItem::new("Move up", Command::Heading(h, HeadingOp::MoveUp))
                .icon(Icon::ArrowUp)
                .hint(key(&format!("{alt}↑"))),
            MenuItem::new("Move down", Command::Heading(h, HeadingOp::MoveDown))
                .icon(Icon::ArrowDown)
                .hint(key(&format!("{alt}↓"))),
            MenuItem::new("Indent", Command::Heading(h, HeadingOp::Indent))
                .icon(Icon::ChevronRight)
                .hint("Tab"),
            MenuItem::new("Outdent", Command::Heading(h, HeadingOp::Outdent))
                .icon(Icon::ChevronLeft)
                .hint("⇧Tab"),
            MenuItem::new("Fold / unfold", Command::Heading(h, HeadingOp::ToggleFold))
                .icon(Icon::ChevronDown),
            MenuItem::new("Refile…", Command::Heading(h, HeadingOp::Refile)).icon(Icon::ArrowMove),
            MenuItem::new("Archive", Command::Heading(h, HeadingOp::Archive)).icon(Icon::Archive),
        ];
        self.open_menu(anchor, None, items, cx);
    }

    pub(crate) fn open_date_picker(
        &mut self,
        target: DateTarget,
        anchor: PopupAnchor,
        cx: &mut Context<Self>,
    ) {
        let (initial, clearable) = match &target {
            DateTarget::Planning(h, kind) => (meta::planning_date(&self.doc, *h, *kind), true),
            DateTarget::Timestamp(r) => (dates::parse_date(&self.doc.text()[r.clone()]), false),
            DateTarget::Insert(_) => (None, false),
        };
        let selected = initial.unwrap_or_else(dates::today);
        self.popup = Some(Popup::Date {
            anchor,
            target,
            month: first_of_month(selected),
            selected,
            clearable: clearable && initial.is_some(),
        });
        cx.notify();
    }

    fn open_tag_input(
        &mut self,
        h: usize,
        anchor: PopupAnchor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| OrgEditor::single_line("New tag", window, cx));
        let sub = cx.subscribe_in(
            &input,
            window,
            |this, input, event, window, cx| match event {
                EditorEvent::Confirm { .. } => {
                    let tag = input.read(cx).text().trim().replace([' ', ':'], "_");
                    if let Some(Popup::TagInput { heading, .. }) = this.popup.take()
                        && !tag.is_empty()
                    {
                        let mut tags = meta::heading(&this.doc, heading)
                            .map(|h| h.tags.clone())
                            .unwrap_or_default();
                        if !tags.contains(&tag) {
                            tags.push(tag);
                        }
                        let r = meta::set_tags(&this.doc, heading, &tags, this.selection());
                        this.apply_op(r, cx);
                    }
                    this.popup = None;
                    window.focus(&this.focus_handle);
                    cx.notify();
                }
                EditorEvent::Cancel => {
                    this.popup = None;
                    window.focus(&this.focus_handle);
                    cx.notify();
                }
                _ => {}
            },
        );
        window.focus(&input.focus_handle(cx));
        self.popup = Some(Popup::TagInput {
            anchor,
            heading: h,
            input,
            _subscription: sub,
        });
        cx.notify();
    }

    /// Open the slash menu for a `/` just typed at `start`.
    pub(crate) fn open_slash(&mut self, start: usize, cx: &mut Context<Self>) {
        self.popup = Some(Popup::Slash { start, selected: 0 });
        cx.notify();
    }

    pub(crate) fn close_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup.take().is_some() {
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    /// The slash menu's current query (text typed after `/`), or `None` if it no longer applies.
    fn slash_query(&self, start: usize) -> Option<&str> {
        let text = self.doc.text();
        let cursor = self.cursor_offset();
        if text.as_bytes().get(start) != Some(&b'/')
            || cursor <= start
            || !self.selected_range.is_empty()
        {
            return None;
        }
        let q = &text[start + 1..cursor];
        (!q.contains(char::is_whitespace) && q.len() <= 24).then_some(q)
    }

    /// Re-validate popups after the text changed.
    pub(crate) fn popup_after_change(&mut self) {
        match &self.popup {
            Some(Popup::Slash { start, .. }) => {
                let ok = self
                    .slash_query(*start)
                    .is_some_and(|q| !slash_matches(q).is_empty());
                if !ok {
                    self.popup = None;
                }
            }
            Some(Popup::TagInput { .. }) => {}
            Some(_) => self.popup = None,
            None => {}
        }
    }

    // ---- keyboard -----------------------------------------------------------------------

    /// Handle a navigation key for the open popup. Returns true when consumed.
    pub(crate) fn popup_key(
        &mut self,
        key: NavKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        enum Then {
            Nothing,
            Close,
            Run(Command),
            PickDate(NaiveDate),
            Slash(usize, usize),
        }
        let slash_len = match &self.popup {
            Some(Popup::Slash { start, .. }) => self
                .slash_query(*start)
                .map_or(0, |q| slash_matches(q).len()),
            _ => 0,
        };
        let then = match self.popup.as_mut() {
            None | Some(Popup::TagInput { .. }) => return false,
            Some(Popup::Menu {
                items, selected, ..
            }) => {
                let n = items.len().max(1);
                match key {
                    NavKey::Up => {
                        *selected = (*selected + n - 1) % n;
                        Then::Nothing
                    }
                    NavKey::Down => {
                        *selected = (*selected + 1) % n;
                        Then::Nothing
                    }
                    NavKey::Enter => Then::Run(items[*selected].command.clone()),
                    NavKey::Escape => Then::Close,
                    NavKey::Left | NavKey::Right => return false,
                }
            }
            Some(Popup::Date {
                selected, month, ..
            }) => {
                let step = match key {
                    NavKey::Left => -1,
                    NavKey::Right => 1,
                    NavKey::Up => -7,
                    NavKey::Down => 7,
                    NavKey::Enter => 0,
                    NavKey::Escape => 99,
                };
                match step {
                    0 => Then::PickDate(*selected),
                    99 => Then::Close,
                    _ => {
                        *selected += Duration::days(step);
                        *month = first_of_month(*selected);
                        Then::Nothing
                    }
                }
            }
            Some(Popup::Slash { start, selected }) => {
                let n = slash_len;
                match key {
                    NavKey::Up if n > 0 => {
                        *selected = (*selected + n - 1) % n;
                        Then::Nothing
                    }
                    NavKey::Down if n > 0 => {
                        *selected = (*selected + 1) % n;
                        Then::Nothing
                    }
                    NavKey::Enter => Then::Slash(*start, *selected),
                    NavKey::Escape => Then::Close,
                    _ => return false,
                }
            }
        };
        match then {
            Then::Nothing => {}
            Then::Close => self.popup = None,
            Then::Run(cmd) => {
                self.popup = None;
                self.run_command(cmd, window, cx);
            }
            Then::PickDate(d) => self.pick_date(d, cx),
            Then::Slash(start, i) => self.run_slash(start, i, window, cx),
        }
        cx.notify();
        true
    }

    // ---- running commands ---------------------------------------------------------------

    pub(crate) fn run_command(
        &mut self,
        cmd: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sel = self.selection();
        // Anchor follow-up popups where the current one was.
        let anchor = self.last_popup_anchor;
        match cmd {
            Command::SetTodo(h, state) => {
                let r = meta::set_todo(&self.doc, h, state.as_deref(), sel);
                self.apply_op(r, cx);
            }
            Command::SetPriority(h, p) => {
                let r = meta::set_priority(&self.doc, h, p, sel);
                self.apply_op(r, cx);
            }
            Command::RemoveTag(h, tag) => {
                let tags: Vec<String> = meta::heading(&self.doc, h)
                    .map(|h| h.tags.iter().filter(|t| **t != tag).cloned().collect())
                    .unwrap_or_default();
                let r = meta::set_tags(&self.doc, h, &tags, sel);
                self.apply_op(r, cx);
            }
            Command::AddTag(h) => self.open_tag_input(h, anchor, window, cx),
            Command::AddExistingTag(h, tag) => {
                let mut tags = meta::heading(&self.doc, h)
                    .map(|h| h.tags.clone())
                    .unwrap_or_default();
                tags.push(tag);
                let r = meta::set_tags(&self.doc, h, &tags, sel);
                self.apply_op(r, cx);
            }
            Command::TodoMenu(h) => self.open_todo_menu(h, anchor, cx),
            Command::PriorityMenu(h) => self.open_priority_menu(h, anchor, cx),
            Command::TagsMenu(h) => self.open_tags_menu(h, anchor, cx),
            Command::PickDate(target) => self.open_date_picker(target, anchor, cx),
            Command::Heading(h, op) => {
                let at = Selection::cursor(h);
                let r = match op {
                    HeadingOp::MoveUp => ops::move_subtree(&self.doc, at, true),
                    HeadingOp::MoveDown => ops::move_subtree(&self.doc, at, false),
                    HeadingOp::Indent => ops::demote(&self.doc, at, true),
                    HeadingOp::Outdent => ops::promote(&self.doc, at, true),
                    HeadingOp::NewBelow => ops::insert_heading(&self.doc, at),
                    HeadingOp::ToggleFold => {
                        if let Some(heading) = meta::heading(&self.doc, h).cloned() {
                            self.toggle_fold_heading(&heading, cx);
                        }
                        return;
                    }
                    // Handled by the app: put the cursor on the heading and send the action up.
                    HeadingOp::Refile | HeadingOp::Archive => {
                        self.set_selection(at);
                        let action: Box<dyn gpui::Action> = match op {
                            HeadingOp::Refile => Box::new(crate::Refile),
                            _ => Box::new(crate::ArchiveSubtree),
                        };
                        window.dispatch_action(action, cx);
                        return;
                    }
                };
                self.apply_op(r, cx);
            }
        }
    }

    fn pick_date(&mut self, d: NaiveDate, cx: &mut Context<Self>) {
        let Some(Popup::Date { target, .. }) = self.popup.take() else {
            return;
        };
        let sel = self.selection();
        let r = match target {
            DateTarget::Planning(h, kind) => meta::set_planning(&self.doc, h, kind, Some(d), sel),
            DateTarget::Timestamp(range) => meta::set_timestamp_date(&self.doc, range, d, sel),
            DateTarget::Insert(at) => {
                let ts = dates::active_timestamp(d);
                Some(ops::OpResult {
                    selection: Selection::cursor(at + ts.len()),
                    tx: Transaction::replace(at..at, ts),
                })
            }
        };
        self.apply_op(r, cx);
        cx.notify();
    }

    fn clear_date(&mut self, cx: &mut Context<Self>) {
        if let Some(Popup::Date {
            target: DateTarget::Planning(h, kind),
            ..
        }) = self.popup.take()
        {
            let r = meta::set_planning(&self.doc, h, kind, None, self.selection());
            self.apply_op(r, cx);
        }
        cx.notify();
    }

    /// Replace `/query` with the chosen slash item.
    fn run_slash(
        &mut self,
        start: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(query) = self.slash_query(start) else {
            self.popup = None;
            return;
        };
        let Some((kind, _)) = slash_matches(query).get(index).copied() else {
            return;
        };
        let end = start + 1 + query.len();
        self.popup = None;
        let text = self.doc.text();
        let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
        let at_line_start = text[line_start..start].trim().is_empty();
        let level = self
            .doc
            .headings()
            .iter()
            .rev()
            .find(|h| h.range.start <= start && start < h.range.end.max(h.range.start + 1))
            .map_or(1, |h| h.level);
        let heading_start = self
            .doc
            .headings()
            .iter()
            .rev()
            .find(|h| h.range.start <= start && (start < h.range.end || h.range.end == text.len()))
            .map(|h| h.range.start);

        // (text to insert in place of "/query", cursor offset within it)
        let line = |s: String, cursor: usize| -> (String, usize) {
            if at_line_start {
                (s, cursor)
            } else {
                (format!("{eol}{s}"), eol.len() + cursor)
            }
        };
        let (insert, cursor) = match kind {
            SlashKind::Heading => {
                let stars = "*".repeat(level);
                line(format!("{stars} "), level + 1)
            }
            SlashKind::Subheading => {
                let stars = "*".repeat(level + 1);
                line(format!("{stars} "), level + 2)
            }
            SlashKind::Todo => {
                let stars = "*".repeat(level);
                line(format!("{stars} TODO "), level + 6)
            }
            SlashKind::Checklist => line("- [ ] ".into(), 6),
            SlashKind::Bullets => line("- ".into(), 2),
            SlashKind::Numbered => line("1. ".into(), 3),
            SlashKind::Table => line(format!("|   |   |{eol}|---+---|{eol}|   |   |"), 2),
            SlashKind::Code => line(format!("#+begin_src {eol}{eol}#+end_src"), 12),
            SlashKind::Quote => line(
                format!("#+begin_quote{eol}{eol}#+end_quote"),
                13 + eol.len(),
            ),
            SlashKind::Divider => line("-----".into(), 5),
            SlashKind::Link => ("[[]]".into(), 2),
            SlashKind::Date | SlashKind::Schedule | SlashKind::Deadline => (String::new(), 0),
        };
        let before = self.selection();
        let tx = Transaction::replace(start..end, insert.clone());
        let after = Selection::cursor(start + cursor);
        self.doc.apply(tx, before, after, EditOrigin::Other);
        self.doc.seal_undo_group();
        self.set_selection(after);
        self.after_change(cx);

        // Items that continue in a date picker.
        let anchor = self.anchor_below(start..start);
        match (kind, heading_start) {
            (SlashKind::Date, _) => {
                self.open_date_picker(DateTarget::Insert(start), anchor, cx);
            }
            (SlashKind::Schedule, Some(h)) => {
                self.open_date_picker(DateTarget::Planning(h, Planning::Scheduled), anchor, cx);
            }
            (SlashKind::Deadline, Some(h)) => {
                self.open_date_picker(DateTarget::Planning(h, Planning::Deadline), anchor, cx);
            }
            _ => {}
        }
        let _ = window;
    }

    // ---- rendering ----------------------------------------------------------------------

    pub(crate) fn render_popup(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        // A slash menu whose `/` text is gone (cursor moved away, etc.) closes.
        if let Some(Popup::Slash { start, .. }) = &self.popup
            && self
                .slash_query(*start)
                .is_none_or(|q| slash_matches(q).is_empty())
        {
            self.popup = None;
        }
        let popup = self.popup.as_ref()?;
        let (anchor, body): (PopupAnchor, AnyElement) = match popup {
            Popup::Menu {
                anchor,
                title,
                items,
                selected,
            } => (
                *anchor,
                self.render_menu(title.clone(), items.clone(), *selected, cx),
            ),
            Popup::Date {
                anchor,
                month,
                selected,
                clearable,
                ..
            } => (*anchor, self.render_date(*month, *selected, *clearable, cx)),
            Popup::TagInput { anchor, input, .. } => (
                *anchor,
                panel(cx)
                    .p_2()
                    .w(px(220.))
                    .child(aster_ui::components::text_box(true, cx).child(input.clone()))
                    .into_any_element(),
            ),
            Popup::Slash { start, selected } => {
                let start = *start;
                let selected = *selected;
                let query = self.slash_query(start)?.to_string();
                let items: Vec<MenuItem> = slash_matches(&query)
                    .into_iter()
                    .map(|(kind, name)| {
                        MenuItem::new(name, Command::Heading(0, HeadingOp::ToggleFold))
                            .icon(slash_icon(kind))
                    })
                    .collect();
                let anchor = self.anchor_below(start..start);
                (anchor, self.render_slash(start, items, selected, cx))
            }
        };
        self.last_popup_anchor = anchor;
        Some(
            deferred(anchor.place(anchored()).child(body))
                .with_priority(1)
                .into_any_element(),
        )
    }

    fn render_menu(
        &self,
        title: Option<SharedString>,
        items: Vec<MenuItem>,
        selected: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        panel(cx)
            .min_w(px(200.))
            .children(title.map(|t| {
                div()
                    .px_3()
                    .pt_1()
                    .pb_0p5()
                    .text_size(px(12.))
                    .text_color(cx.theme().colors.text_secondary)
                    .child(t)
            }))
            .children(items.into_iter().enumerate().map(|(i, item)| {
                let cmd = item.command.clone();
                menu_row(
                    i,
                    item.label,
                    item.hint,
                    item.checked,
                    item.icon,
                    i == selected,
                    cx,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.popup = None;
                        this.run_command(cmd.clone(), window, cx);
                        cx.notify();
                    }),
                )
            }))
            .into_any_element()
    }

    fn render_slash(
        &self,
        start: usize,
        items: Vec<MenuItem>,
        selected: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        panel(cx)
            .min_w(px(220.))
            .child(
                div()
                    .px_3()
                    .pt_1()
                    .text_size(px(12.))
                    .text_color(cx.theme().colors.text_secondary)
                    .child("Insert"),
            )
            .children(items.into_iter().enumerate().map(|(i, item)| {
                menu_row(i, item.label, None, false, item.icon, i == selected, cx).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.run_slash(start, i, window, cx);
                        cx.notify();
                    }),
                )
            }))
            .into_any_element()
    }

    fn render_date(
        &self,
        month: NaiveDate,
        selected: NaiveDate,
        clearable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let today = dates::today();
        let c = cx.theme().colors;
        let title = month.format("%B %Y").to_string();
        let lead = month.weekday().num_days_from_monday() as i64;
        let grid_start = month - Duration::days(lead);
        let nav = |id: &'static str, label: &'static str, delta: i32| {
            div()
                .id(id)
                .px_2()
                .rounded(aster_ui::RADIUS_CONTROL)
                .cursor_pointer()
                .hover(move |s| s.bg(c.subtle_secondary))
                .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(Popup::Date {
                            month, selected, ..
                        }) = this.popup.as_mut()
                        {
                            *month = add_months(*month, delta);
                            *selected = *month;
                        }
                        cx.notify();
                    }),
                )
        };
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .px_1()
            .child(nav("date-prev", "‹", -1))
            .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(title))
            .child(nav("date-next", "›", 1));
        let weekdays = div()
            .flex()
            .children(["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].map(|d| {
                div()
                    .w(px(32.))
                    .text_center()
                    .text_size(px(11.))
                    .text_color(c.text_tertiary)
                    .child(d)
            }));
        let weeks = (0..6).map(|w| {
            div().flex().children((0..7).map(|d| {
                let day = grid_start + Duration::days(w * 7 + d);
                let in_month = day.month() == month.month();
                let is_selected = day == selected;
                let is_today = day == today;
                div()
                    .id(("day", (w * 7 + d) as usize))
                    .w(px(32.))
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(aster_ui::RADIUS_CONTROL)
                    .cursor_pointer()
                    .text_size(px(13.))
                    .text_color(if is_selected {
                        c.text_on_accent
                    } else if in_month {
                        c.text_primary
                    } else {
                        c.text_disabled
                    })
                    .when(is_selected, |s| s.bg(c.accent))
                    .when(is_today && !is_selected, |s| {
                        s.border_1().border_color(c.accent)
                    })
                    .when(!is_selected, |s| s.hover(move |s| s.bg(c.subtle_secondary)))
                    .child(day.day().to_string())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.pick_date(day, cx);
                        }),
                    )
            }))
        });
        let footer = div()
            .flex()
            .justify_between()
            .pt_1()
            .child(
                div()
                    .id("date-today")
                    .px_2()
                    .rounded(aster_ui::RADIUS_CONTROL)
                    .cursor_pointer()
                    .text_color(c.accent)
                    .hover(move |s| s.bg(c.subtle_secondary))
                    .child("Today")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.pick_date(today, cx);
                        }),
                    ),
            )
            .when(clearable, |f| {
                f.child(
                    div()
                        .id("date-clear")
                        .px_2()
                        .rounded(aster_ui::RADIUS_CONTROL)
                        .cursor_pointer()
                        .text_color(c.critical)
                        .hover(move |s| s.bg(c.subtle_secondary))
                        .child("Clear")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.clear_date(cx);
                            }),
                        ),
                )
            });
        panel(cx)
            .p_2()
            .text_size(px(14.))
            .child(header)
            .child(weekdays)
            .children(weeks)
            .child(footer)
            .into_any_element()
    }
}

fn panel(cx: &gpui::App) -> gpui::Div {
    aster_ui::components::flyout(cx)
}

/// A menu row (docs/DESIGN.md §7): 28 px, 4 px radius, keyboard selection uses the hover fill,
/// optional accent check mark and a caption shortcut hint.
fn menu_row(
    i: usize,
    label: SharedString,
    hint: Option<SharedString>,
    checked: bool,
    glyph: Option<Icon>,
    selected: bool,
    cx: &gpui::App,
) -> gpui::Stateful<gpui::Div> {
    let c = cx.theme().colors;
    div()
        .id(("menu-item", i))
        .h(px(28.))
        .px_2()
        .rounded(aster_ui::RADIUS_CONTROL)
        .flex()
        .items_center()
        .gap_3()
        .cursor_pointer()
        .when(selected, |s| s.bg(c.subtle_secondary))
        .when(!selected, |s| s.hover(move |s| s.bg(c.subtle_secondary)))
        // Leading column: a check mark for the current choice, otherwise the item's icon.
        .child(
            div()
                .w(px(16.))
                .flex_shrink_0()
                .map(|d| match (checked, glyph) {
                    (true, _) => d.child(icon(Icon::Checkmark, px(16.), c.accent)),
                    (false, Some(g)) => d.child(icon(g, px(16.), c.text_secondary)),
                    (false, None) => d,
                }),
        )
        .child(div().flex_1().child(label))
        .children(hint.map(|h| {
            div()
                .text_size(aster_ui::ramp::CAPTION.size)
                .text_color(c.text_tertiary)
                .child(h)
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_filtering() {
        assert_eq!(slash_matches("").len(), SLASH_ITEMS.len());
        let kinds: Vec<_> = slash_matches("tab").into_iter().map(|(k, _)| k).collect();
        assert_eq!(kinds, [SlashKind::Table]);
        let first = |q: &str| slash_matches(q)[0].0;
        assert_eq!(first("ta"), SlashKind::Table);
        assert_eq!(first("to"), SlashKind::Todo);
        assert_eq!(first("check"), SlashKind::Checklist);
        assert_eq!(first("task"), SlashKind::Todo);
        assert!(
            slash_matches("todo")
                .iter()
                .any(|(k, _)| *k == SlashKind::Todo)
        );
        assert!(slash_matches("zzz").is_empty());
    }

    #[test]
    fn month_math() {
        let d = NaiveDate::from_ymd_opt(2026, 12, 15).unwrap();
        assert_eq!(
            add_months(d, 1),
            NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()
        );
        assert_eq!(
            add_months(d, -12),
            NaiveDate::from_ymd_opt(2025, 12, 1).unwrap()
        );
    }
}
