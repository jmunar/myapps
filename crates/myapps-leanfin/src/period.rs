//! The time window shared by the Balance and Breakdown tabs.
//!
//! A window is a whole number of *complete* calendar periods — 30 days, 10
//! weeks, 6 or 12 months — optionally followed by the period we are currently
//! living in. "Complete" is what makes the windows comparable: on the 4th of
//! September a 12m window runs from the 1st of September a year earlier to the
//! 31st of August, twelve months that each had every one of their days, and
//! the running month is an opt-in thirteenth bucket rather than a short bar
//! that quietly drags every average down.
//!
//! The running bucket carries a `weight` below 1 — the fraction of it that has
//! already happened — so an average per period can divide by the periods it
//! actually observed. The weight counts today as elapsed, because today's
//! transactions are already in the numerator.

use chrono::{Datelike, Duration, NaiveDate};

/// The granularity of one bar in a chart.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bucket {
    Day,
    Week,
    Month,
}

/// A window the selector can be set to, shortest first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Window {
    D30,
    W10,
    M6,
    M12,
}

/// Every window, shortest to longest. The selector steps through this list.
pub const WINDOWS: [Window; 4] = [Window::D30, Window::W10, Window::M6, Window::M12];

/// What both tabs open on.
pub const DEFAULT_WINDOW: Window = Window::W10;

/// Whether the running period is charted unless the user says otherwise.
pub const DEFAULT_INCLUDE_CURRENT: bool = true;

impl Window {
    /// The wire form: what the query string carries and the selector shows.
    pub fn key(self) -> &'static str {
        match self {
            Window::D30 => "30d",
            Window::W10 => "10w",
            Window::M6 => "6m",
            Window::M12 => "12m",
        }
    }

    /// Lenient on purpose: these arrive in a URL a person can edit, so junk
    /// falls back to the default rather than a 400.
    pub fn parse(s: Option<&str>) -> Window {
        s.and_then(|s| WINDOWS.iter().copied().find(|w| w.key() == s))
            .unwrap_or(DEFAULT_WINDOW)
    }

    pub fn bucket(self) -> Bucket {
        match self {
            Window::D30 => Bucket::Day,
            Window::W10 => Bucket::Week,
            Window::M6 | Window::M12 => Bucket::Month,
        }
    }

    /// How many complete periods the window holds.
    pub fn count(self) -> i64 {
        match self {
            Window::D30 => 30,
            Window::W10 => 10,
            Window::M6 => 6,
            Window::M12 => 12,
        }
    }
}

/// Last day of the month `day` falls in.
fn month_end(day: NaiveDate) -> NaiveDate {
    let (y, m) = (day.year(), day.month());
    let first_of_next = if m == 12 {
        NaiveDate::from_ymd_opt(y + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(y, m + 1, 1)
    };
    // Every (year, month) pair here comes from a real date, so the 1st of the
    // following month always exists.
    first_of_next.unwrap_or(day) - Duration::days(1)
}

/// Last day of the month `n` months before the one ending at `end`.
fn month_end_back(end: NaiveDate, n: i64) -> NaiveDate {
    let months = end.year() as i64 * 12 + (end.month() as i64 - 1) - n;
    let year = months.div_euclid(12) as i32;
    let month = months.rem_euclid(12) as u32 + 1;
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(end);
    month_end(first)
}

impl Bucket {
    /// End of the period `day` falls in. Weeks are Monday–Sunday.
    fn end_of(self, day: NaiveDate) -> NaiveDate {
        match self {
            Bucket::Day => day,
            Bucket::Week => day + Duration::days(6 - day.weekday().num_days_from_monday() as i64),
            Bucket::Month => month_end(day),
        }
    }

    /// End of the period `n` periods before the one ending at `end`.
    fn end_back(self, end: NaiveDate, n: i64) -> NaiveDate {
        match self {
            Bucket::Day => end - Duration::days(n),
            Bucket::Week => end - Duration::days(7 * n),
            Bucket::Month => month_end_back(end, n),
        }
    }

    /// First day of the period ending at `end`.
    fn start_of(self, end: NaiveDate) -> NaiveDate {
        match self {
            Bucket::Day => end,
            Bucket::Week => end - Duration::days(6),
            Bucket::Month => end.with_day(1).unwrap_or(end),
        }
    }
}

/// One bar's worth of calendar.
pub struct Period {
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// 1.0 for a period that has run its course; the fraction elapsed for the
    /// running one, so a per-period average can divide by it.
    pub weight: f64,
}

/// The buckets a window covers, chronological, oldest first.
pub struct Periods {
    pub bucket: Bucket,
    pub periods: Vec<Period>,
}

impl Periods {
    /// First day covered — the start of the oldest bucket.
    pub fn start(&self) -> NaiveDate {
        self.periods
            .first()
            .map(|p| p.start)
            .unwrap_or_else(|| chrono::Utc::now().date_naive())
    }

    pub fn ends(&self) -> Vec<String> {
        self.periods.iter().map(|p| iso(p.end)).collect()
    }

    pub fn starts(&self) -> Vec<String> {
        self.periods.iter().map(|p| iso(p.start)).collect()
    }

    pub fn weights(&self) -> Vec<f64> {
        self.periods.iter().map(|p| p.weight).collect()
    }
}

pub fn iso(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// The buckets of `window` as of `today`, with the running period appended
/// when `include_current`.
pub fn periods(window: Window, include_current: bool, today: NaiveDate) -> Periods {
    let bucket = window.bucket();
    // The period holding today is by definition still running, so the newest
    // *complete* one is the one before it.
    let current_end = bucket.end_of(today);
    let last_complete_end = bucket.end_back(current_end, 1);

    let mut periods: Vec<Period> = (0..window.count())
        .rev()
        .map(|back| {
            let end = bucket.end_back(last_complete_end, back);
            Period {
                start: bucket.start_of(end),
                end,
                weight: 1.0,
            }
        })
        .collect();

    if include_current {
        let start = bucket.start_of(current_end);
        let total = (current_end - start).num_days() + 1;
        let elapsed = (today - start).num_days() + 1;
        periods.push(Period {
            start,
            end: current_end,
            weight: (elapsed as f64 / total as f64).clamp(0.0, 1.0),
        });
    }

    Periods { bucket, periods }
}

/// Which bucket a transaction dated `date` belongs to, as that bucket's end
/// date. Dates outside the window still map to *some* end date; callers drop
/// what the window does not list.
pub fn bucket_end_for(bucket: Bucket, date: NaiveDate) -> NaiveDate {
    bucket.end_of(date)
}

/// The window selector: one box you step through, plus the toggle that adds
/// the running period. `on_change` is the name of a page-level JS function
/// called with `(windowKey, includeCurrent)`.
pub fn render_selector(
    window: Window,
    include_current: bool,
    on_change: &str,
    lang: myapps_core::i18n::Lang,
) -> String {
    let t = super::i18n::t(lang);
    let pressed = if include_current { "true" } else { "false" };
    let active = if include_current {
        " lf-window-now-active"
    } else {
        ""
    };
    format!(
        r#"<div class="lf-window" data-window="{key}" data-current="{current}" data-onchange="{on_change}">
            <div class="lf-window-box" role="spinbutton" tabindex="0"
                 aria-label="{win_label}" aria-valuetext="{key}">
                <span class="lf-window-value">{key}</span>
                <span class="lf-window-steps">
                    <button type="button" class="lf-window-step" data-step="longer"
                            aria-label="{longer}" title="{longer}">&#9650;</button>
                    <button type="button" class="lf-window-step" data-step="shorter"
                            aria-label="{shorter}" title="{shorter}">&#9660;</button>
                </span>
            </div>
            <button type="button" class="lf-window-now{active}" aria-pressed="{pressed}"
                    aria-label="{current_label}" title="{current_label}">+</button>
        </div>"#,
        key = window.key(),
        current = if include_current { "1" } else { "0" },
        win_label = t.win_label,
        longer = t.win_longer,
        shorter = t.win_shorter,
        current_label = t.win_current,
    )
}

/// Behaviour for every selector on the page. Inlined into a `<script>` rather
/// than served separately so it shares the page's cache entry.
pub const SELECTOR_JS: &str = include_str!("../static/window-selector.js");

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn a_twelve_month_window_starts_a_year_and_a_month_back() {
        // The example from the brief: on 4 Sep 2026 the window opens on the
        // 1st of September a year earlier, and September 2026 is the extra,
        // running bucket.
        let p = periods(Window::M12, true, d("2026-09-04"));
        assert_eq!(p.periods.len(), 13);
        assert_eq!(p.start(), d("2025-09-01"));
        assert_eq!(p.periods[0].end, d("2025-09-30"));
        assert_eq!(p.periods[11].end, d("2026-08-31"));
        assert_eq!(p.periods[12].start, d("2026-09-01"));
        assert_eq!(p.periods[12].end, d("2026-09-30"));
    }

    #[test]
    fn without_the_current_period_every_bucket_is_complete() {
        let p = periods(Window::M12, false, d("2026-09-04"));
        assert_eq!(p.periods.len(), 12);
        assert_eq!(p.start(), d("2025-09-01"));
        assert_eq!(p.periods.last().unwrap().end, d("2026-08-31"));
        assert!(p.weights().iter().all(|w| *w == 1.0));
    }

    #[test]
    fn the_running_period_is_weighted_by_the_days_gone_by() {
        let p = periods(Window::M12, true, d("2026-09-04"));
        let w = *p.weights().last().unwrap();
        // 4 of September's 30 days have happened, today included.
        assert!((w - 4.0 / 30.0).abs() < 1e-9, "weight was {w}");
        assert!(p.weights()[..12].iter().all(|w| *w == 1.0));
    }

    #[test]
    fn weeks_run_monday_to_sunday_and_the_last_complete_one_ends_before_today() {
        // 2026-09-04 is a Friday; its week ends Sunday 2026-09-06.
        let p = periods(Window::W10, true, d("2026-09-04"));
        assert_eq!(p.periods.len(), 11);
        assert_eq!(p.periods.last().unwrap().start, d("2026-08-31"));
        assert_eq!(p.periods.last().unwrap().end, d("2026-09-06"));
        assert_eq!(p.periods[9].end, d("2026-08-30"));
        assert_eq!(p.start(), d("2026-06-22"));
        // Mon–Fri of seven days.
        let w = *p.weights().last().unwrap();
        assert!((w - 5.0 / 7.0).abs() < 1e-9, "weight was {w}");
    }

    #[test]
    fn a_sunday_does_not_complete_its_own_week() {
        // 2026-09-06 is a Sunday: the week ending that day is still running.
        let p = periods(Window::W10, true, d("2026-09-06"));
        assert_eq!(p.periods.last().unwrap().end, d("2026-09-06"));
        assert_eq!(p.periods[9].end, d("2026-08-30"));
        assert_eq!(*p.weights().last().unwrap(), 1.0);
    }

    #[test]
    fn thirty_days_end_yesterday_and_today_is_the_extra_bucket() {
        let p = periods(Window::D30, true, d("2026-09-04"));
        assert_eq!(p.periods.len(), 31);
        assert_eq!(p.start(), d("2026-08-05"));
        assert_eq!(p.periods[29].end, d("2026-09-03"));
        assert_eq!(p.periods[30].end, d("2026-09-04"));
        // A day bucket is one day long, so today weighs a whole one.
        assert_eq!(*p.weights().last().unwrap(), 1.0);
    }

    #[test]
    fn a_month_window_crosses_the_year_boundary() {
        let p = periods(Window::M6, false, d("2026-02-10"));
        assert_eq!(p.start(), d("2025-08-01"));
        assert_eq!(p.periods[0].end, d("2025-08-31"));
        assert_eq!(p.periods.last().unwrap().end, d("2026-01-31"));
    }

    #[test]
    fn a_leap_february_keeps_its_twenty_ninth() {
        let p = periods(Window::M6, true, d("2028-02-29"));
        let last = p.periods.last().unwrap();
        assert_eq!(last.start, d("2028-02-01"));
        assert_eq!(last.end, d("2028-02-29"));
        assert_eq!(last.weight, 1.0);
        assert_eq!(p.periods[5].end, d("2028-01-31"));
    }

    #[test]
    fn a_junk_window_key_falls_back_to_the_default() {
        assert_eq!(Window::parse(Some("12m")), Window::M12);
        assert_eq!(Window::parse(Some("365d")), DEFAULT_WINDOW);
        assert_eq!(Window::parse(None), DEFAULT_WINDOW);
    }
}
