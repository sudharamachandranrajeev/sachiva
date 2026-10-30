//! Counts for targets and tallies, and the periods they add up over.
//!
//! Every +1, +N or correction is its own row and is never changed or
//! deleted, so a total can always be rebuilt and nothing is lost. Totals
//! per period are worked out from the rows, never stored.

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};
use serde::{Deserialize, Serialize};

use crate::model::{Kind, Span, Task};

/// Most a single entry may add or take away.
pub const MAX_DELTA: i64 = 10_000;
pub const MAX_NOTE: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Count {
    pub id: i64,
    pub task_id: String,
    pub at: Timestamp,
    pub delta: i64,
    #[serde(default)]
    pub note: String,
}

/// The first day of the period that holds `day`. Weeks start on Monday.
pub fn period_start(span: Span, day: Date) -> Date {
    match span {
        Span::Day => day,
        Span::Week => day - (day.weekday().to_monday_zero_offset() as i64).days(),
        Span::Month => day.first_of_month(),
    }
}

/// The first day of the next period.
pub fn next_start(span: Span, start: Date) -> Date {
    match span {
        Span::Day => start.tomorrow().unwrap_or(start),
        Span::Week => start.checked_add(7.days()).unwrap_or(start),
        Span::Month => start.first_of_month().checked_add(1.month()).unwrap_or(start),
    }
}

/// Whether `day` is where a period of this span begins.
pub fn is_start(span: Span, day: Date) -> bool {
    period_start(span, day) == day
}

/// Period starts from the one holding `from` to the one holding `to`,
/// at most `limit` of them (the newest kept).
pub fn starts(span: Span, from: Date, to: Date, limit: usize) -> Vec<Date> {
    let mut v = Vec::new();
    let mut d = period_start(span, from);
    let end = period_start(span, to);
    while d <= end {
        v.push(d);
        let n = next_start(span, d);
        if n <= d {
            break;
        }
        d = n;
    }
    if v.len() > limit {
        v.drain(..v.len() - limit);
    }
    v
}

/// Midnight at the start of `day`, local time, as an instant. Days that
/// skip midnight (a clock change at 00:00) start at the first valid time.
pub fn instant(day: Date, tz: &TimeZone) -> Timestamp {
    day.to_zoned(tz.clone()).map(|z| z.timestamp()).unwrap_or(Timestamp::MAX)
}

pub fn local_day(t: Timestamp, tz: &TimeZone) -> Date {
    t.to_zoned(tz.clone()).date()
}

/// Sum of counts with `from <= at < to`.
pub fn total(counts: &[Count], from: Timestamp, to: Timestamp) -> i64 {
    counts.iter().filter(|c| c.at >= from && c.at < to).map(|c| c.delta).sum()
}

/// Totals for each period start, in order.
pub fn per_period(counts: &[Count], span: Span, starts: &[Date], tz: &TimeZone) -> Vec<i64> {
    starts.iter().map(|&s| total(counts, instant(s, tz), instant(next_start(span, s), tz))).collect()
}

/// Where a counter or target stands now, for its dashboard card.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub task_id: String,
    pub done: i64,
    pub goal: Option<u32>,
    pub unit: String,
    /// The period this counts over; `None` for a one-off target (all time).
    pub span: Option<Span>,
    pub today: i64,
    pub week: i64,
    pub month: i64,
    /// For targets: how much should be done by now at an even pace.
    pub expected: Option<i64>,
    /// For targets: behind the even pace, with time running out.
    pub behind: bool,
    /// For repeating targets: periods met out of the last ones (not counting this one).
    pub met: Option<(usize, usize)>,
    /// One line in plain words: "12 of 30 this week, 3 days left".
    pub line: String,
}

/// How many past periods "met in N of the last M" looks at.
pub const MET_WINDOW: usize = 12;

/// `None` for tasks that are not targets or tallies.
pub fn progress(task: &Task, counts: &[Count], now: Timestamp, tz: &TimeZone) -> Option<Progress> {
    let (goal, unit, span) = match &task.kind {
        Kind::Target { goal, unit, every } => (Some(*goal), unit.clone(), *every),
        Kind::Tally { unit } => (None, unit.clone(), None),
        _ => return None,
    };
    let today_d = local_day(now, tz);
    let sum_span = |s: Span| {
        let st = period_start(s, today_d);
        total(counts, instant(st, tz), instant(next_start(s, st), tz))
    };
    let (today, week, month) = (sum_span(Span::Day), sum_span(Span::Week), sum_span(Span::Month));
    let unit_word = if unit.trim().is_empty() { "done".to_string() } else { unit.trim().to_string() };

    let (done, from, to) = match (span, &task.kind) {
        (Some(s), _) => {
            let st = period_start(s, today_d);
            let (a, b) = (instant(st, tz), instant(next_start(s, st), tz));
            (total(counts, a, b), Some(a), Some(b))
        }
        (None, Kind::Target { .. }) => {
            // A one-off target counts everything since it was set, up to its date.
            (counts.iter().map(|c| c.delta).sum(), Some(task.created_at), task.due)
        }
        _ => (counts.iter().map(|c| c.delta).sum(), None, None),
    };

    let mut expected = None;
    let mut behind = false;
    if let (Some(g), Some(a), Some(b)) = (goal, from, to)
        && b > a
    {
        let frac = ((now.as_second() - a.as_second()) as f64 / (b.as_second() - a.as_second()) as f64).clamp(0.0, 1.0);
        let e = (f64::from(g) * frac).floor() as i64;
        expected = Some(e);
        // Behind: under the even pace, short of the goal, and in the last
        // third of the period, when there is still time to catch up.
        behind = done < e && done < i64::from(g) && frac >= 2.0 / 3.0 && now < b;
    }

    let met = match (goal, span) {
        (Some(g), Some(s)) => {
            let cur = period_start(s, today_d);
            let first = local_day(task.created_at, tz);
            let mut past = Vec::new();
            let mut d = cur;
            while past.len() < MET_WINDOW {
                let prev = prev_start(s, d);
                if prev >= d || next_start(s, prev) <= period_start(s, first) {
                    break;
                }
                past.push(prev);
                d = prev;
            }
            let totals = per_period(counts, s, &past, tz);
            Some((totals.iter().filter(|&&t| t >= i64::from(g)).count(), past.len()))
        }
        _ => None,
    };

    let line = match (goal, span) {
        (Some(g), Some(s)) => {
            let left = to.map(|b| left_words(now, b, s)).unwrap_or_default();
            format!("{done} of {g} {}{left}", period_words(s))
        }
        (Some(g), None) => {
            let by = task.due.map(|d| format!(" by {}", d.to_zoned(tz.clone()).strftime("%-d %b"))).unwrap_or_default();
            format!("{done} of {g}{by}")
        }
        _ => format!("{today} today, {week} this week, {month} this month"),
    };
    Some(Progress {
        task_id: task.id.clone(),
        done,
        goal,
        unit: unit_word,
        span,
        today,
        week,
        month,
        expected,
        behind,
        met,
        line,
    })
}

fn prev_start(span: Span, start: Date) -> Date {
    match span {
        Span::Day => start.yesterday().unwrap_or(start),
        Span::Week => start.checked_sub(7.days()).unwrap_or(start),
        Span::Month => start.checked_sub(1.month()).unwrap_or(start).first_of_month(),
    }
}

fn period_words(s: Span) -> &'static str {
    match s {
        Span::Day => "today",
        Span::Week => "this week",
        Span::Month => "this month",
    }
}

fn left_words(now: Timestamp, end: Timestamp, s: Span) -> String {
    let secs = end.as_second() - now.as_second();
    if secs <= 0 {
        return String::new();
    }
    match s {
        Span::Day => format!(", {} left", crate::priority::span(secs)),
        _ => {
            let days = (secs + 86_399) / 86_400;
            if days == 1 { ", last day".into() } else { format!(", {days} days left") }
        }
    }
}

/// Checks a new entry. `total` is the task's all-time total before it.
pub fn check_entry(delta: i64, note: &str, total: i64) -> Result<(), String> {
    if delta == 0 || delta.abs() > MAX_DELTA {
        return Err(format!("Add or take away between 1 and {MAX_DELTA} at a time."));
    }
    if total + delta < 0 {
        return Err("That would take the count below zero.".into());
    }
    if note.chars().count() > MAX_NOTE {
        return Err(format!("Keep the note under {MAX_NOTE} characters."));
    }
    Ok(())
}

/// How reports name a period: "4 Nov 2026", "w/c 2 Nov 2026", "Nov 2026".
pub fn label(span: Span, start: Date) -> String {
    match span {
        Span::Day => start.strftime("%-d %b %Y").to_string(),
        Span::Week => format!("w/c {}", start.strftime("%-d %b %Y")),
        Span::Month => start.strftime("%b %Y").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn tz() -> TimeZone {
        TimeZone::get("Europe/London").unwrap()
    }

    fn c(at: &str, delta: i64) -> Count {
        Count { id: 0, task_id: "t".into(), at: at.parse().unwrap(), delta, note: String::new() }
    }

    #[test]
    fn periods() {
        // 2026-11-04 is a Wednesday.
        assert_eq!(period_start(Span::Week, date(2026, 11, 4)), date(2026, 11, 2));
        assert_eq!(period_start(Span::Month, date(2026, 11, 30)), date(2026, 11, 1));
        assert_eq!(next_start(Span::Month, date(2026, 1, 1)), date(2026, 2, 1));
        assert_eq!(next_start(Span::Week, date(2026, 12, 28)), date(2027, 1, 4));
        assert!(is_start(Span::Week, date(2026, 11, 2)) && !is_start(Span::Week, date(2026, 11, 3)));
        let s = starts(Span::Month, date(2026, 1, 15), date(2026, 4, 2), 100);
        assert_eq!(s, vec![date(2026, 1, 1), date(2026, 2, 1), date(2026, 3, 1), date(2026, 4, 1)]);
        assert_eq!(starts(Span::Day, date(2026, 1, 1), date(2026, 1, 10), 3).len(), 3);
    }

    #[test]
    fn clock_change_days_count_whole() {
        // The clocks go back on 25 Oct 2026: that Sunday has 25 hours.
        let counts =
            [c("2026-10-25T00:30:00+01:00", 1), c("2026-10-25T23:30:00+00:00", 1), c("2026-10-26T00:10:00+00:00", 1)];
        let t = per_period(&counts, Span::Day, &[date(2026, 10, 25)], &tz());
        assert_eq!(t, vec![2]);
    }

    #[test]
    fn weekly_target_adds_partial_progress() {
        let now: Timestamp = "2026-11-06T15:00:00Z".parse().unwrap(); // Friday
        let mut t = Task::new("Run", "2026-10-01T09:00:00Z".parse().unwrap());
        t.kind = Kind::Target { goal: 6, unit: "km".into(), every: Some(Span::Week) };
        let counts = [
            c("2026-11-02T10:00:00Z", 3),
            c("2026-11-04T10:00:00Z", 3),
            c("2026-10-27T10:00:00Z", 6), // last week: met
            c("2026-10-20T10:00:00Z", 2), // the week before: not met
        ];
        let p = progress(&t, &counts, now, &tz()).unwrap();
        assert_eq!(p.done, 6);
        assert_eq!(p.week, 6);
        assert!(!p.behind);
        assert!(p.line.starts_with("6 of 6 this week"), "{}", p.line);
        let (met, of) = p.met.unwrap();
        assert_eq!(met, 1);
        assert!(of >= 2);
    }

    #[test]
    fn behind_pace_late_in_the_period() {
        let now: Timestamp = "2026-11-07T12:00:00Z".parse().unwrap(); // Saturday
        let mut t = Task::new("Bookings", "2026-11-01T09:00:00Z".parse().unwrap());
        t.kind = Kind::Target { goal: 30, unit: "bookings".into(), every: Some(Span::Week) };
        let p = progress(&t, &[c("2026-11-03T10:00:00Z", 5)], now, &tz()).unwrap();
        assert!(p.behind);
        assert!(p.expected.unwrap() > 5);
    }

    #[test]
    fn tally_totals() {
        let now: Timestamp = "2026-11-04T15:00:00Z".parse().unwrap();
        let mut t = Task::new("Calls", now);
        t.kind = Kind::Tally { unit: "calls".into() };
        let counts = [
            c("2026-11-04T09:00:00Z", 4),
            c("2026-11-03T09:00:00Z", 2),
            c("2026-10-30T09:00:00Z", 1),
            c("2026-11-04T10:00:00Z", -1),
        ];
        let p = progress(&t, &counts, now, &tz()).unwrap();
        assert_eq!((p.today, p.week, p.month, p.done), (3, 5, 5, 6));
        assert_eq!(p.line, "3 today, 5 this week, 5 this month");
    }

    #[test]
    fn entries() {
        assert!(check_entry(1, "", 0).is_ok());
        assert!(check_entry(0, "", 0).is_err());
        assert!(check_entry(-2, "", 1).is_err());
        assert!(check_entry(MAX_DELTA + 1, "", 0).is_err());
    }
}
