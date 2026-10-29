//! Recurring tasks. Rules run on local wall-clock time, so a 09:00 task stays
//! at 09:00 across a clock change.

use jiff::civil::{Date, DateTime};
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};

use crate::model::{Repeat, Status, Task, Weekday};

/// Mark `task` done at `now`. Returns the finished task and, for a recurring
/// task, the next one (a new id, open, checklist cleared).
pub fn complete(task: &Task, now: Timestamp, tz: &TimeZone) -> (Task, Option<Task>) {
    let mut done = task.clone();
    done.status = Status::Done { at: now };
    done.updated_at = now;
    let next = task.repeat.as_ref().and_then(|r| {
        let anchor = task.deadline();
        let due = next_after(r, anchor, now, tz)?;
        let mut n = task.clone();
        n.id = uuid::Uuid::now_v7().to_string();
        if let (Some(a), Some(old)) = (task.appointment, anchor) {
            n.appointment = Some(a + (due.as_second() - old.as_second()).seconds());
        }
        if task.due.is_some() || task.appointment.is_none() {
            n.due = match (task.due, anchor) {
                (Some(d), Some(old)) => Some(d + (due.as_second() - old.as_second()).seconds()),
                _ => Some(due),
            };
        }
        n.status = Status::Open;
        n.checklist.iter_mut().for_each(|c| c.done = false);
        n.snoozed_until = None;
        n.created_at = now;
        n.updated_at = now;
        Some(n)
    });
    (done, next)
}

/// The next occurrence for a task whose current deadline is `anchor`, done at `done_at`.
/// A fixed schedule skips occurrences already in the past, so finishing late
/// does not leave a pile of overdue copies; finishing early moves one step.
pub fn next_after(rule: &Repeat, anchor: Option<Timestamp>, done_at: Timestamp, tz: &TimeZone) -> Option<Timestamp> {
    let local = |t: Timestamp| t.to_zoned(tz.clone()).datetime();
    if let Repeat::AfterDone { days } = rule {
        let time = local(anchor.unwrap_or(done_at)).time();
        let date = local(done_at).date().checked_add(i64::from(*days).days()).ok()?;
        return to_ts(date.to_datetime(time), tz);
    }
    let start = local(anchor.unwrap_or(done_at));
    let mut cur = start;
    for _ in 0..2000 {
        cur = step(rule, cur)?;
        let ts = to_ts(cur, tz)?;
        if ts > done_at {
            return Some(ts);
        }
    }
    None
}

fn step(rule: &Repeat, cur: DateTime) -> Option<DateTime> {
    let d = cur.date();
    let date = match rule {
        Repeat::Daily { every } => d.checked_add(i64::from((*every).max(1)).days()).ok()?,
        Repeat::Weekly { every, days } => {
            let every = i64::from((*every).max(1));
            if days.is_empty() {
                d.checked_add((7 * every).days()).ok()?
            } else {
                // Only weeks a multiple of `every` after the current one count.
                let base = monday_of(d);
                (1..=7 * every + 7)
                    .filter_map(|i| d.checked_add(i.days()).ok())
                    .find(|x| days.contains(&weekday(*x)) && weeks_between(base, monday_of(*x)) % every == 0)?
            }
        }
        Repeat::Monthly { every, day } => {
            let first = d.first_of_month().checked_add(i64::from((*every).max(1)).months()).ok()?;
            let day = (*day).clamp(1, 31) as i8;
            first.with().day(day.min(first.days_in_month())).build().ok()?
        }
        Repeat::AfterDone { .. } => return None,
    };
    Some(date.to_datetime(cur.time()))
}

fn monday_of(d: Date) -> Date {
    d.checked_sub(i64::from(d.weekday().to_monday_zero_offset()).days()).unwrap_or(d)
}

fn weeks_between(a: Date, b: Date) -> i64 {
    let days = |d: Date| {
        d.to_datetime(jiff::civil::Time::midnight())
            .to_zoned(TimeZone::UTC)
            .map(|z| z.timestamp().as_second() / 86_400)
            .unwrap_or(0)
    };
    (days(b) - days(a)).div_euclid(7)
}

fn weekday(d: Date) -> Weekday {
    use jiff::civil::Weekday as W;
    match d.weekday() {
        W::Monday => Weekday::Mon,
        W::Tuesday => Weekday::Tue,
        W::Wednesday => Weekday::Wed,
        W::Thursday => Weekday::Thu,
        W::Friday => Weekday::Fri,
        W::Saturday => Weekday::Sat,
        W::Sunday => Weekday::Sun,
    }
}

fn to_ts(dt: DateTime, tz: &TimeZone) -> Option<Timestamp> {
    tz.to_ambiguous_zoned(dt).compatible().ok().map(|z| z.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CheckItem;

    fn london() -> TimeZone {
        TimeZone::get("Europe/London").unwrap()
    }
    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }
    fn local(t: Timestamp) -> String {
        t.to_zoned(london()).strftime("%a %Y-%m-%d %H:%M").to_string()
    }

    #[test]
    fn daily_keeps_wall_time_across_clock_change() {
        // 09:00 BST on Sat 24 Oct 2026; clocks go back on Sun 25 Oct.
        let r = Repeat::Daily { every: 1 };
        let n = next_after(&r, Some(at("2026-10-24T08:00:00Z")), at("2026-10-24T08:30:00Z"), &london()).unwrap();
        assert_eq!(local(n), "Sun 2026-10-25 09:00");
        assert_eq!(n, at("2026-10-25T09:00:00Z"));
    }

    #[test]
    fn late_completion_skips_missed_occurrences() {
        let r = Repeat::Weekly { every: 1, days: vec![] };
        let n = next_after(&r, Some(at("2026-10-01T09:00:00Z")), at("2026-10-20T12:00:00Z"), &london()).unwrap();
        assert_eq!(local(n), "Thu 2026-10-22 10:00");
    }

    #[test]
    fn early_completion_moves_one_step() {
        let r = Repeat::Daily { every: 2 };
        let n = next_after(&r, Some(at("2026-11-10T09:00:00Z")), at("2026-11-02T09:00:00Z"), &london()).unwrap();
        assert_eq!(local(n), "Thu 2026-11-12 09:00");
    }

    #[test]
    fn weekly_on_days_every_second_week() {
        let r = Repeat::Weekly { every: 2, days: vec![Weekday::Mon, Weekday::Fri] };
        let tz = london();
        let mut due = at("2026-11-02T10:00:00Z"); // Mon
        let mut seen = vec![];
        for _ in 0..4 {
            due = next_after(&r, Some(due), due, &tz).unwrap();
            seen.push(local(due));
        }
        assert_eq!(
            seen,
            ["Fri 2026-11-06 10:00", "Mon 2026-11-16 10:00", "Fri 2026-11-20 10:00", "Mon 2026-11-30 10:00"]
        );
    }

    #[test]
    fn monthly_31st_does_not_drift() {
        let r = Repeat::Monthly { every: 1, day: 31 };
        let tz = london();
        let mut due = at("2027-01-31T10:00:00Z");
        let mut seen = vec![];
        for _ in 0..3 {
            due = next_after(&r, Some(due), due, &tz).unwrap();
            seen.push(local(due)[4..14].to_string());
        }
        assert_eq!(seen, ["2027-02-28", "2027-03-31", "2027-04-30"]);
    }

    #[test]
    fn after_done_counts_from_completion() {
        let r = Repeat::AfterDone { days: 3 };
        let n = next_after(&r, Some(at("2026-11-01T09:00:00Z")), at("2026-11-05T15:00:00Z"), &london()).unwrap();
        assert_eq!(local(n), "Sun 2026-11-08 09:00");
    }

    #[test]
    fn complete_makes_a_fresh_copy() {
        let now = at("2026-11-05T15:00:00Z");
        let mut t = Task::new("Stock check", at("2026-11-01T08:00:00Z"));
        t.due = Some(at("2026-11-05T17:00:00Z"));
        t.repeat = Some(Repeat::Weekly { every: 1, days: vec![] });
        t.checklist = vec![CheckItem { text: "gloves".into(), done: true }];
        let (done, next) = complete(&t, now, &london());
        assert!(done.is_done());
        let next = next.unwrap();
        assert_ne!(next.id, t.id);
        assert_eq!(next.due, Some(at("2026-11-12T17:00:00Z")));
        assert!(!next.checklist[0].done);
        assert_eq!(next.status, Status::Open);
    }
}
