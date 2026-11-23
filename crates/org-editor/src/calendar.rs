//! A month calendar for picking a date: the editor's date picker and the capture box share it.

use std::rc::Rc;

use chrono::{Datelike, Duration, NaiveDate};
use gpui::{App, Div, MouseButton, Window, div, prelude::*, px};
use org_ui::ActiveTheme;

type Handler<T> = Rc<dyn Fn(T, &mut Window, &mut App)>;

/// What the calendar shows and does. The caller keeps `month` and `selected` and updates them
/// from the callbacks.
pub struct Calendar {
    /// Any day in the month shown.
    pub month: NaiveDate,
    pub selected: Option<NaiveDate>,
    pub today: NaiveDate,
    /// Called with ±1 for the previous/next month.
    pub on_month: Handler<i32>,
    pub on_pick: Handler<NaiveDate>,
    /// Shows a "Clear" button when set.
    pub on_clear: Option<Handler<()>>,
}

/// The first day of the month `months` after `d`'s month.
pub fn add_months(d: NaiveDate, months: i32) -> NaiveDate {
    let total = d.year() * 12 + d.month0() as i32 + months;
    NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1)
        .expect("valid month")
}

impl Calendar {
    pub fn render(self, cx: &App) -> Div {
        let c = cx.theme().colors;
        let month = self.month.with_day(1).expect("valid day");
        let title = month.format("%B %Y").to_string();
        let lead = month.weekday().num_days_from_monday() as i64;
        let grid_start = month - Duration::days(lead);
        let button = |id: &'static str| {
            div()
                .id(id)
                .px_2()
                .rounded(org_ui::RADIUS_CONTROL)
                .cursor_pointer()
                .hover(move |s| s.bg(c.subtle_secondary))
        };
        let nav = |id: &'static str, label: &'static str, delta: i32| {
            let on_month = self.on_month.clone();
            button(id)
                .child(label)
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    on_month(delta, window, cx);
                })
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
                let is_selected = self.selected == Some(day);
                let is_today = day == self.today;
                let on_pick = self.on_pick.clone();
                div()
                    .id(("day", (w * 7 + d) as usize))
                    .w(px(32.))
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(org_ui::RADIUS_CONTROL)
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
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        on_pick(day, window, cx);
                    })
            }))
        });
        let today = self.today;
        let on_pick = self.on_pick.clone();
        let footer = div()
            .flex()
            .justify_between()
            .pt_1()
            .child(
                button("date-today")
                    .text_color(c.accent)
                    .child("Today")
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        on_pick(today, window, cx);
                    }),
            )
            .children(self.on_clear.map(|on_clear| {
                button("date-clear")
                    .text_color(c.critical)
                    .child("Clear")
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        on_clear((), window, cx);
                    })
            }));
        div()
            .text_size(px(14.))
            .child(header)
            .child(weekdays)
            .children(weeks)
            .child(footer)
    }
}
