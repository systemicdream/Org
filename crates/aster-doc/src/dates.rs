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

#[cfg(test)]
mod tests {
    use super::*;

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
