//! Org timestamps: `<2026-11-22 Sun>`, `<2026-11-22 Sun 10:00 +1w>`, `[2026-11-22 Sun]`.

use std::ops::Range;

use chrono::{Datelike, NaiveDate, Weekday};

pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

fn day_name(d: NaiveDate) -> &'static str {
    match d.weekday() {
        Weekday::Mon => "Mon",
        Weekday::Tue => "Tue",
        Weekday::Wed => "Wed",
        Weekday::Thu => "Thu",
        Weekday::Fri => "Fri",
        Weekday::Sat => "Sat",
        Weekday::Sun => "Sun",
    }
}

/// `2026-11-22 Sun`
pub fn format_date(d: NaiveDate) -> String {
    format!("{} {}", d.format("%Y-%m-%d"), day_name(d))
}

/// An active timestamp for `d`: `<2026-11-22 Sun>`.
pub fn active_timestamp(d: NaiveDate) -> String {
    format!("<{}>", format_date(d))
}

/// The date in a timestamp like `<2026-11-22 Sun 10:00>`.
pub fn parse_date(timestamp: &str) -> Option<NaiveDate> {
    let inner = timestamp.trim_start_matches(['<', '[']);
    NaiveDate::parse_from_str(inner.get(..10)?, "%Y-%m-%d").ok()
}

/// Byte range of the `YYYY-MM-DD Day` part inside a timestamp's text.
fn date_part(timestamp: &str) -> Option<Range<usize>> {
    let start = 1;
    parse_date(timestamp)?;
    let mut end = start + 10;
    // Optional day name after a space.
    let rest = &timestamp[end..];
    if let Some(name) = rest.strip_prefix(' ') {
        let n = name.chars().take_while(|c| c.is_alphabetic()).count();
        if n > 0 {
            end += 1 + name[..n].len();
        }
    }
    Some(start..end)
}

/// Replace the date in `timestamp`, keeping its brackets, time, repeater and warning.
pub fn with_date(timestamp: &str, d: NaiveDate) -> Option<String> {
    let r = date_part(timestamp)?;
    let mut s = timestamp.to_string();
    s.replace_range(r, &format_date(d));
    Some(s)
}

/// An inactive timestamp with the current time: `[2026-11-22 Sun 14:05]`.
pub fn inactive_now() -> String {
    let now = chrono::Local::now();
    format!(
        "[{} {}]",
        format_date(now.date_naive()),
        now.format("%H:%M")
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatUnit {
    Day,
    Week,
    Month,
    Year,
}

/// A repeater like `+1w` (also `++1w` and `.+1w`, treated alike for display).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repeater {
    pub every: u32,
    pub unit: RepeatUnit,
}

impl Repeater {
    /// The occurrence after `d`.
    pub fn next(&self, d: NaiveDate) -> NaiveDate {
        let n = self.every.max(1);
        match self.unit {
            RepeatUnit::Day => d + chrono::Duration::days(n as i64),
            RepeatUnit::Week => d + chrono::Duration::weeks(n as i64),
            RepeatUnit::Month => d.checked_add_months(chrono::Months::new(n)).unwrap_or(d),
            RepeatUnit::Year => d
                .checked_add_months(chrono::Months::new(12 * n))
                .unwrap_or(d),
        }
    }
}

/// A parsed Org timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timestamp {
    pub date: NaiveDate,
    pub active: bool,
    /// `10:00` or `10:00-11:30`.
    pub time: Option<String>,
    pub repeater: Option<Repeater>,
    /// Days of advance warning for deadlines (`-3d`).
    pub warning_days: Option<i64>,
}

impl Timestamp {
    /// Parse `<2026-11-22 Sun 10:00 +1w -3d>` or `[2026-11-22]`.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let active = s.starts_with('<');
        let close = if active { '>' } else { ']' };
        let inner = s.strip_prefix(['<', '['])?;
        let inner = &inner[..inner.find(close)?];
        let date = NaiveDate::parse_from_str(inner.get(..10)?, "%Y-%m-%d").ok()?;
        let mut time = None;
        let mut repeater = None;
        let mut warning_days = None;
        for part in inner[10..].split_whitespace() {
            if part.chars().next().is_some_and(|c| c.is_ascii_digit()) && part.contains(':') {
                time = Some(part.to_string());
            } else if let Some(rest) = part
                .strip_prefix(".+")
                .or_else(|| part.strip_prefix("++"))
                .or_else(|| part.strip_prefix('+'))
            {
                repeater = parse_interval(rest).map(|(every, unit)| Repeater { every, unit });
            } else if let Some(rest) = part.strip_prefix("--").or_else(|| part.strip_prefix('-')) {
                warning_days = parse_interval(rest).map(|(n, unit)| {
                    n as i64
                        * match unit {
                            RepeatUnit::Day => 1,
                            RepeatUnit::Week => 7,
                            RepeatUnit::Month => 30,
                            RepeatUnit::Year => 365,
                        }
                });
            }
        }
        Some(Self {
            date,
            active,
            time,
            repeater,
            warning_days,
        })
    }

    /// Dates this timestamp falls on within `from..=to` (repeating ones may hit several).
    pub fn occurrences(&self, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        let mut out = Vec::new();
        let mut d = self.date;
        match self.repeater {
            None => {
                if d >= from && d <= to {
                    out.push(d);
                }
            }
            Some(r) => {
                // Repeats start from the base date; skip ahead to the window.
                let mut guard = 0;
                while d < from && guard < 100_000 {
                    d = r.next(d);
                    guard += 1;
                }
                while d <= to && out.len() < 400 {
                    out.push(d);
                    d = r.next(d);
                }
            }
        }
        out
    }
}

fn parse_interval(s: &str) -> Option<(u32, RepeatUnit)> {
    let unit = match s.chars().last()? {
        'd' => RepeatUnit::Day,
        'w' => RepeatUnit::Week,
        'm' => RepeatUnit::Month,
        'y' => RepeatUnit::Year,
        _ => return None,
    };
    let n = s[..s.len() - 1].parse().ok()?;
    Some((n, unit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_timestamps() {
        let t = Timestamp::parse("<2026-11-22 Sun 10:00-11:30 .+1w -3d>").unwrap();
        assert_eq!(t.date, NaiveDate::from_ymd_opt(2026, 11, 22).unwrap());
        assert!(t.active);
        assert_eq!(t.time.as_deref(), Some("10:00-11:30"));
        assert_eq!(
            t.repeater,
            Some(Repeater {
                every: 1,
                unit: RepeatUnit::Week
            })
        );
        assert_eq!(t.warning_days, Some(3));
        let t = Timestamp::parse("[2026-01-31 Sat]").unwrap();
        assert!(!t.active);
        assert_eq!(t.time, None);
        assert!(Timestamp::parse("<nope>").is_none());
    }

    #[test]
    fn repeating_occurrences() {
        let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        let weekly = Timestamp::parse("<2026-11-02 Mon +1w>").unwrap();
        assert_eq!(
            weekly.occurrences(d(11, 10), d(11, 30)),
            [d(11, 16), d(11, 23), d(11, 30)]
        );
        let monthly = Timestamp::parse("<2026-01-31 Sat +1m>").unwrap();
        assert_eq!(monthly.occurrences(d(3, 1), d(3, 31)).len(), 1);
        let once = Timestamp::parse("<2026-11-22 Sun>").unwrap();
        assert_eq!(once.occurrences(d(11, 1), d(11, 21)), []);
    }

    #[test]
    fn format_and_replace() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        assert_eq!(active_timestamp(d), "<2026-11-22 Sun>");
        assert_eq!(parse_date("<2026-11-22 Sun 10:00>"), Some(d));
        assert_eq!(parse_date("[2026-11-22]"), Some(d));
        let next = NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
        assert_eq!(
            with_date("<2026-11-22 Sun 10:00 +1w -2d>", next).unwrap(),
            "<2026-12-01 Tue 10:00 +1w -2d>"
        );
        assert_eq!(with_date("[2026-11-22]", next).unwrap(), "[2026-12-01 Tue]");
        assert_eq!(parse_date("<nope>"), None);
    }
}
