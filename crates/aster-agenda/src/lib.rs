//! Agenda views over the workspace index, following Org's defaults:
//!
//! - **Day/week:** entries scheduled, due, or timestamped on each day (repeaters expanded).
//!   Today also lists overdue scheduled items, overdue deadlines, and deadlines coming up
//!   within their warning period (14 days unless the timestamp says otherwise).
//! - **TODO list:** every open TODO.
//! - **Match:** Org's tag/property query syntax, e.g. `+work-home TODO="NEXT" PRIORITY="A"`.
//!
//! Tags are inherited from parent headings and `#+FILETAGS`, as in Org.

use std::path::PathBuf;

use aster_index::{HeadingEntry, Index};
use chrono::NaiveDate;

/// Default advance warning for deadlines, in days (`org-deadline-warning-days`).
pub const DEADLINE_WARNING_DAYS: i64 = 14;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    /// Scheduled for this day (`late == 0`) or still open `late` days after (shown today).
    Scheduled { late: i64 },
    /// Due on this day (`days == 0`), in `days` (upcoming), or overdue (`days < 0`).
    Deadline { days: i64 },
    /// A plain active timestamp on this day.
    Timestamp,
    /// No date (TODO list and match results).
    Entry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaItem {
    pub path: PathBuf,
    pub rel: String,
    pub offset: usize,
    pub title: String,
    pub level: usize,
    pub todo: Option<String>,
    pub done: bool,
    pub priority: Option<char>,
    /// The heading's own tags.
    pub tags: Vec<String>,
    /// `10:00` or `10:00-11:00`, when the date has a time.
    pub time: Option<String>,
    pub kind: ItemKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaDay {
    pub date: NaiveDate,
    pub items: Vec<AgendaItem>,
}

/// Every heading with its full set of tags (own, inherited from parents, and file tags).
fn headings_with_tags(index: &Index) -> Vec<(&aster_index::FileEntry, &HeadingEntry, Vec<String>)> {
    let mut out = Vec::new();
    for f in index.files().filter(|f| !f.conflict) {
        let mut stack: Vec<(usize, Vec<String>)> = Vec::new();
        for h in &f.headings {
            while stack.last().is_some_and(|(level, _)| *level >= h.level) {
                stack.pop();
            }
            let mut all: Vec<String> = f.file_tags.clone();
            for (_, tags) in &stack {
                all.extend(tags.iter().cloned());
            }
            all.extend(h.tags.iter().cloned());
            all.dedup();
            stack.push((h.level, h.tags.clone()));
            out.push((f, h, all));
        }
    }
    out
}

/// The title without active timestamps (the agenda already shows the date).
fn clean_title(title: &str) -> String {
    let mut out = String::new();
    let mut rest = title;
    while let Some(i) = rest.find('<') {
        let Some(end) = rest[i..].find('>') else {
            break;
        };
        let candidate = &rest[i..i + end + 1];
        let is_date =
            candidate.len() > 11 && candidate[1..11].chars().filter(|c| *c == '-').count() == 2;
        out.push_str(&rest[..i]);
        if !is_date {
            out.push_str(candidate);
        }
        rest = &rest[i + end + 1..];
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn item(
    f: &aster_index::FileEntry,
    h: &HeadingEntry,
    time: Option<String>,
    kind: ItemKind,
) -> AgendaItem {
    AgendaItem {
        path: f.path.clone(),
        rel: f.rel.clone(),
        offset: h.offset,
        title: clean_title(&h.title),
        level: h.level,
        todo: h.todo.clone(),
        done: h.done,
        priority: h.priority,
        tags: h.tags.clone(),
        time,
        kind,
    }
}

fn kind_rank(k: &ItemKind) -> i64 {
    match k {
        ItemKind::Deadline { days } if *days < 0 => 0,
        ItemKind::Scheduled { late } if *late > 0 => 1,
        ItemKind::Deadline { days: 0 } => 2,
        ItemKind::Scheduled { .. } => 3,
        ItemKind::Timestamp => 4,
        ItemKind::Deadline { .. } => 5,
        ItemKind::Entry => 6,
    }
}

/// Within a kind: sooner deadlines and later (more overdue) scheduled items first.
fn urgency(k: &ItemKind) -> i64 {
    match k {
        ItemKind::Deadline { days } => *days,
        ItemKind::Scheduled { late } => -late,
        _ => 0,
    }
}

/// Sort: timed entries first by time, then by kind (overdue first), urgency, priority, title.
fn sort_items(items: &mut [AgendaItem]) {
    items.sort_by(|a, b| {
        let time = |i: &AgendaItem| i.time.clone().unwrap_or_else(|| "~".into());
        time(a)
            .cmp(&time(b))
            .then(a.done.cmp(&b.done))
            .then(kind_rank(&a.kind).cmp(&kind_rank(&b.kind)))
            .then(urgency(&a.kind).cmp(&urgency(&b.kind)))
            .then(a.priority.unwrap_or('Z').cmp(&b.priority.unwrap_or('Z')))
            .then(a.title.cmp(&b.title))
    });
}

/// Day-by-day agenda for `days` days starting at `start`.
pub fn agenda(index: &Index, start: NaiveDate, days: i64, today: NaiveDate) -> Vec<AgendaDay> {
    let end = start + chrono::Duration::days(days.max(1) - 1);
    let mut out: Vec<AgendaDay> = (0..days.max(1))
        .map(|i| AgendaDay {
            date: start + chrono::Duration::days(i),
            items: Vec::new(),
        })
        .collect();
    let slot = |d: NaiveDate| (d - start).num_days() as usize;
    let today_in_range = today >= start && today <= end;

    for (f, h, _) in headings_with_tags(index) {
        if let Some(ts) = &h.scheduled {
            for d in ts.occurrences(start, end) {
                out[slot(d)].items.push(item(
                    f,
                    h,
                    ts.time.clone(),
                    ItemKind::Scheduled { late: 0 },
                ));
            }
            // Only open tasks become overdue; plain events just happen on their days.
            let open_task = h.todo.is_some() && !h.done;
            if today_in_range && open_task && ts.date < today {
                let late = (today - ts.date).num_days();
                out[slot(today)]
                    .items
                    .push(item(f, h, None, ItemKind::Scheduled { late }));
            }
        }
        if let Some(ts) = &h.deadline {
            for d in ts.occurrences(start, end) {
                if !(d == today && today_in_range) {
                    out[slot(d)].items.push(item(
                        f,
                        h,
                        ts.time.clone(),
                        ItemKind::Deadline { days: 0 },
                    ));
                }
            }
            if today_in_range && h.todo.is_some() && !h.done {
                let days = (ts.date - today).num_days();
                let warning = ts.warning_days.unwrap_or(DEADLINE_WARNING_DAYS);
                if days <= warning {
                    let time = (days == 0).then(|| ts.time.clone()).flatten();
                    out[slot(today)]
                        .items
                        .push(item(f, h, time, ItemKind::Deadline { days }));
                }
            }
        }
        for ts in &h.timestamps {
            for d in ts.occurrences(start, end) {
                out[slot(d)]
                    .items
                    .push(item(f, h, ts.time.clone(), ItemKind::Timestamp));
            }
        }
    }
    for day in &mut out {
        sort_items(&mut day.items);
    }
    out
}

/// Every open (not done) TODO, highest priority first.
pub fn todo_list(index: &Index) -> Vec<AgendaItem> {
    let mut items: Vec<AgendaItem> = headings_with_tags(index)
        .into_iter()
        .filter(|(_, h, _)| h.todo.is_some() && !h.done)
        .map(|(f, h, _)| item(f, h, None, ItemKind::Entry))
        .collect();
    items.sort_by(|a, b| {
        a.priority
            .unwrap_or('Z')
            .cmp(&b.priority.unwrap_or('Z'))
            .then(a.rel.cmp(&b.rel))
            .then(a.offset.cmp(&b.offset))
    });
    items
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Term {
    Tag(String),
    /// KEY=value (or KEY<>value when `not_equal`).
    Prop {
        key: String,
        value: String,
        not_equal: bool,
    },
}

/// Parse Org match syntax: `+tag`, `-tag`, `tag`, `KEY=value`, `KEY="value"`, `KEY<>value`.
/// Terms may be joined (`+work-home`) or separated by spaces.
fn parse_query(query: &str) -> Vec<(bool, Term)> {
    let mut out = Vec::new();
    let chars: Vec<char> = query.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || c == '&' {
            i += 1;
            continue;
        }
        let include = c != '-';
        if c == '+' || c == '-' {
            i += 1;
        }
        // Key or tag name.
        let start = i;
        while i < chars.len() && (chars[i].is_alphanumeric() || "_@#%".contains(chars[i])) {
            i += 1;
        }
        let name: String = chars[start..i].iter().collect();
        if name.is_empty() {
            i += 1;
            continue;
        }
        let rest: String = chars[i..].iter().collect();
        let op = if rest.starts_with("<>") {
            Some((true, 2))
        } else if rest.starts_with('=') {
            Some((false, 1))
        } else {
            None
        };
        match op {
            Some((not_equal, len)) => {
                i += len;
                let value = if chars.get(i) == Some(&'"') {
                    i += 1;
                    let s = i;
                    while i < chars.len() && chars[i] != '"' {
                        i += 1;
                    }
                    let v: String = chars[s..i].iter().collect();
                    i += 1;
                    v
                } else {
                    let s = i;
                    while i < chars.len() && !chars[i].is_whitespace() {
                        i += 1;
                    }
                    chars[s..i].iter().collect()
                };
                out.push((
                    include,
                    Term::Prop {
                        key: name,
                        value,
                        not_equal,
                    },
                ));
            }
            None => out.push((include, Term::Tag(name))),
        }
    }
    out
}

/// Headings matching an Org tag/property query.
pub fn matches(index: &Index, query: &str) -> Vec<AgendaItem> {
    let terms = parse_query(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let mut items: Vec<AgendaItem> = headings_with_tags(index)
        .into_iter()
        .filter(|(_, h, tags)| {
            terms.iter().all(|(include, term)| {
                let hit = match term {
                    Term::Tag(t) => tags.iter().any(|x| x.eq_ignore_ascii_case(t)),
                    Term::Prop {
                        key,
                        value,
                        not_equal,
                    } => {
                        let actual = match key.to_ascii_uppercase().as_str() {
                            "TODO" => h.todo.clone(),
                            "PRIORITY" => h.priority.map(|p| p.to_string()),
                            "LEVEL" => Some(h.level.to_string()),
                            _ => h.property(key).map(str::to_string),
                        };
                        (actual.as_deref() == Some(value.as_str())) != *not_equal
                    }
                };
                hit == *include
            })
        })
        .map(|(f, h, _)| item(f, h, None, ItemKind::Entry))
        .collect();
    items.sort_by(|a, b| a.rel.cmp(&b.rel).then(a.offset.cmp(&b.offset)));
    items
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn index(files: &[(&str, &str)]) -> Index {
        let mut idx = Index::default();
        for (name, text) in files {
            idx.upsert_text(Path::new(&format!("/w/{name}")), text.to_string());
        }
        idx
    }

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    fn titles(items: &[AgendaItem]) -> Vec<(&str, ItemKind)> {
        items
            .iter()
            .map(|i| (i.title.as_str(), i.kind.clone()))
            .collect()
    }

    const NOTES: &str = "\
* TODO Write report
SCHEDULED: <2026-11-20 Fri>
* TODO Pay rent
DEADLINE: <2026-11-30 Mon>
* TODO Taxes
DEADLINE: <2026-11-25 Wed -3d>
* DONE Old task
SCHEDULED: <2026-11-10 Tue>
* Standup
SCHEDULED: <2026-11-02 Mon 09:30 +1w>
* Dinner <2026-11-23 Mon 19:00>
* Far deadline
DEADLINE: <2027-03-01 Mon>
";

    #[test]
    fn today_shows_overdue_and_upcoming() {
        let idx = index(&[("a.org", NOTES)]);
        let days = agenda(&idx, d(11, 23), 1, d(11, 23));
        assert_eq!(
            titles(&days[0].items),
            [
                ("Standup", ItemKind::Scheduled { late: 0 }),
                ("Dinner", ItemKind::Timestamp),
                ("Write report", ItemKind::Scheduled { late: 3 }),
                ("Taxes", ItemKind::Deadline { days: 2 }),
                ("Pay rent", ItemKind::Deadline { days: 7 }),
            ]
        );
        // "Taxes" warns only 3 days ahead; it appears from the 22nd.
        let days = agenda(&idx, d(11, 22), 1, d(11, 22));
        assert!(titles(&days[0].items).contains(&("Taxes", ItemKind::Deadline { days: 3 })));
    }

    #[test]
    fn week_view_places_entries_on_their_days() {
        let idx = index(&[("a.org", NOTES)]);
        let week = agenda(&idx, d(11, 24), 7, d(11, 24));
        let on = |day: u32| {
            week.iter()
                .find(|w| w.date == d(11, day))
                .map(|w| titles(&w.items))
                .unwrap()
        };
        assert!(on(25).contains(&("Taxes", ItemKind::Deadline { days: 0 })));
        assert!(on(29).is_empty());
        assert!(on(30).iter().any(|(t, _)| *t == "Standup"));
        assert!(
            on(30)
                .iter()
                .any(|(t, k)| *t == "Pay rent" && *k == ItemKind::Deadline { days: 0 })
        );
    }

    #[test]
    fn todo_list_and_matches_with_inheritance() {
        let idx = index(&[
            (
                "w.org",
                "#+TODO: TODO NEXT | DONE\n#+FILETAGS: :work:\n* NEXT [#A] Ship it :urgent:\n:PROPERTIES:\n:OWNER: ann\n:END:\n** TODO Sub task\n* TODO At home :home:\n",
            ),
            ("p.org", "* TODO [#B] Groceries :errand:\n* DONE Finished\n"),
        ]);
        let todos: Vec<_> = todo_list(&idx).into_iter().map(|i| i.title).collect();
        assert_eq!(todos, ["Ship it", "Groceries", "Sub task", "At home"]);

        let m =
            |q: &str| -> Vec<String> { matches(&idx, q).into_iter().map(|i| i.title).collect() };
        assert_eq!(m("+work-home"), ["Ship it", "Sub task"]);
        assert_eq!(m("urgent"), ["Ship it", "Sub task"]); // inherited by the child
        assert_eq!(m("work TODO=\"NEXT\""), ["Ship it"]);
        assert_eq!(m("OWNER=ann"), ["Ship it"]);
        assert_eq!(m("errand PRIORITY=B"), ["Groceries"]);
        assert_eq!(m("TODO<>TODO -work"), ["Finished"]);
    }
}
