//! When to remind. One pure function lists the reminder moments of a task in a
//! time window; the desktop scheduler fires the latest one per task each tick,
//! and Android schedules the next one as an exact alarm.
//!
//! The rules answer the two loudest complaints about other apps: reminders
//! that are easy to miss (urgent ones repeat until you act) and reminders that
//! nag at the wrong time (none right after you edit, none outside working hours
//! for repeats).

use jiff::civil::Time;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

use crate::model::{Status, Task};
use crate::priority::Priority;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// No reminder within this many minutes of creating or editing a task.
    pub quiet_after_edit_min: i64,
    /// Repeat reminders for overdue jobs and chases this often.
    pub repeat_every_min: i64,
    /// Urgent jobs repeat this often.
    pub urgent_every_min: i64,
    /// Score at which a job counts as urgent.
    pub urgent_score: u8,
    /// Repeats only happen between these times, Monday to Friday.
    pub work_start: Time,
    pub work_end: Time,
    pub weekends: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            quiet_after_edit_min: 10,
            repeat_every_min: 120,
            urgent_every_min: 10,
            urgent_score: 85,
            work_start: Time::constant(8, 0, 0, 0),
            work_end: Time::constant(18, 0, 0, 0),
            weekends: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    HeadsUp,
    Due,
    Overdue,
    Chase,
    Snoozed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub task_id: String,
    pub at: Timestamp,
    pub kind: Kind,
    /// Urgent reminders stay on screen until acted on.
    pub urgent: bool,
    pub title: String,
    pub body: String,
}

/// Reminder moments of `task` in `(from, to]`, earliest first.
pub fn between(
    task: &Task,
    prio: &Priority,
    from: Timestamp,
    to: Timestamp,
    s: &Settings,
    tz: &TimeZone,
) -> Vec<Reminder> {
    if task.is_done() || to <= from {
        return Vec::new();
    }
    let urgent = prio.score >= s.urgent_score;
    let every = SignedDuration::from_mins(if urgent { s.urgent_every_min } else { s.repeat_every_min }.max(1));
    let mut moments: Vec<(Timestamp, Kind)> = Vec::new();

    let (start, repeat_kind) = match &task.status {
        Status::Waiting { chase: Some(c), .. } => {
            moments.push((*c, Kind::Chase));
            (Some(*c), Kind::Chase)
        }
        Status::Waiting { chase: None, .. } => (None, Kind::Chase),
        _ => match task.deadline() {
            Some(d) => {
                let lead = SignedDuration::from_mins(i64::from(task.effort_min.unwrap_or(45)) + 15);
                moments.push((d - lead, Kind::HeadsUp));
                moments.push((d, Kind::Due));
                // Urgent jobs repeat from the heads-up on; others once overdue.
                (Some(if urgent { d - lead } else { d }), Kind::Overdue)
            }
            None => (None, Kind::Overdue),
        },
    };
    if let Some(start) = start {
        // First repeat after `from`, without walking every past step.
        let gap = (from.as_second() - start.as_second()).max(0) / every.as_secs().max(1);
        let mut t = start + every * (gap as i32);
        while t <= to {
            if t > start && t > from && in_hours(t, s, tz) {
                moments.push((t, if t < task.deadline().unwrap_or(t) { Kind::HeadsUp } else { repeat_kind }));
            }
            t += every;
        }
    }

    let quiet_until = task.updated_at + SignedDuration::from_mins(s.quiet_after_edit_min);
    let snooze = task.snoozed_until.filter(|z| *z > quiet_until);
    let floor = snooze.unwrap_or(quiet_until);
    let mut out: Vec<Reminder> = moments
        .into_iter()
        .filter(|(t, _)| *t > from && *t <= to && *t >= floor)
        .map(|(at, kind)| make(task, prio, at, kind, urgent))
        .collect();
    // A snooze that ends while the job is still pending brings it back once.
    if let Some(z) = snooze
        && z > from
        && z <= to
        && task.deadline().is_none_or(|d| d - SignedDuration::from_mins(60) <= z)
    {
        out.push(make(task, prio, z, Kind::Snoozed, urgent));
    }
    out.sort_by_key(|r| r.at);
    out.dedup_by_key(|r| r.at);
    out
}

/// The next reminder after `now`, looking ahead `days` days (for Android alarms).
pub fn next(task: &Task, prio: &Priority, now: Timestamp, days: i64, s: &Settings, tz: &TimeZone) -> Option<Reminder> {
    between(task, prio, now, now + SignedDuration::from_hours(24 * days), s, tz).into_iter().next()
}

fn make(task: &Task, prio: &Priority, at: Timestamp, kind: Kind, urgent: bool) -> Reminder {
    let title = match kind {
        Kind::HeadsUp => format!("Coming up: {}", task.title),
        Kind::Due => format!("Due now: {}", task.title),
        Kind::Overdue => format!("Overdue: {}", task.title),
        Kind::Chase => format!("Time to chase: {}", task.title),
        Kind::Snoozed => format!("Back from snooze: {}", task.title),
    };
    Reminder { task_id: task.id.clone(), at, kind, urgent, title, body: prio.why() }
}

fn in_hours(t: Timestamp, s: &Settings, tz: &TimeZone) -> bool {
    let z = t.to_zoned(tz.clone());
    let weekday_ok = s.weekends || z.weekday().to_monday_one_offset() <= 5;
    let time = z.time();
    weekday_ok && time >= s.work_start && time < s.work_end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Importance;
    use crate::priority::score;

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }
    fn london() -> TimeZone {
        TimeZone::get("Europe/London").unwrap()
    }
    fn kinds(v: &[Reminder]) -> Vec<(String, Kind)> {
        v.iter().map(|r| (r.at.to_zoned(london()).strftime("%a %H:%M").to_string(), r.kind)).collect()
    }

    fn task() -> Task {
        // Wed 4 Nov 2026 (GMT), due 15:00, 30 minutes' work, made the day before.
        let mut t = Task::new("Chase PO-0419", at("2026-11-03T09:00:00Z"));
        t.due = Some(at("2026-11-04T15:00:00Z"));
        t.effort_min = Some(30);
        t
    }

    #[test]
    fn heads_up_due_then_repeats_in_working_hours() {
        let t = task();
        let p = score(&t, at("2026-11-04T09:00:00Z")).unwrap();
        let r =
            between(&t, &p, at("2026-11-04T09:00:00Z"), at("2026-11-05T12:00:00Z"), &Settings::default(), &london());
        assert_eq!(
            kinds(&r),
            [
                ("Wed 14:15".into(), Kind::HeadsUp),
                ("Wed 15:00".into(), Kind::Due),
                ("Wed 17:00".into(), Kind::Overdue),
                // 19:00 to 07:00 are outside working hours.
                ("Thu 09:00".into(), Kind::Overdue),
                ("Thu 11:00".into(), Kind::Overdue),
            ]
        );
        assert!(!r[0].urgent);
        assert_eq!(r[0].body, p.why());
    }

    #[test]
    fn urgent_repeats_every_ten_minutes_from_heads_up() {
        let mut t = task();
        t.importance = Importance::High;
        let now = at("2026-11-04T14:00:00Z");
        let p = score(&t, now).unwrap();
        assert!(p.score >= 85, "{}", p.score);
        let r = between(&t, &p, now, at("2026-11-04T15:00:00Z"), &Settings::default(), &london());
        let times: Vec<_> = kinds(&r).into_iter().map(|(t, _)| t).collect();
        assert_eq!(times, ["Wed 14:15", "Wed 14:25", "Wed 14:35", "Wed 14:45", "Wed 14:55", "Wed 15:00"]);
        assert!(r.iter().all(|x| x.urgent));
    }

    #[test]
    fn quiet_right_after_editing() {
        let mut t = task();
        t.updated_at = at("2026-11-04T14:10:00Z");
        let p = score(&t, t.updated_at).unwrap();
        let r = between(&t, &p, t.updated_at, at("2026-11-04T14:30:00Z"), &Settings::default(), &london());
        assert!(r.is_empty(), "{:?}", kinds(&r));
        let r = between(&t, &p, t.updated_at, at("2026-11-04T15:00:00Z"), &Settings::default(), &london());
        assert_eq!(kinds(&r), [("Wed 15:00".into(), Kind::Due)]);
    }

    #[test]
    fn snooze_silences_then_returns_once() {
        let mut t = task();
        t.snoozed_until = Some(at("2026-11-04T16:30:00Z"));
        let p = score(&t, at("2026-11-04T14:00:00Z")).unwrap();
        let r =
            between(&t, &p, at("2026-11-04T14:00:00Z"), at("2026-11-04T17:30:00Z"), &Settings::default(), &london());
        assert_eq!(kinds(&r), [("Wed 16:30".into(), Kind::Snoozed), ("Wed 17:00".into(), Kind::Overdue)]);
    }

    #[test]
    fn chase_date_and_done_tasks() {
        let mut t = task();
        t.status = Status::Waiting { on: "supplier".into(), chase: Some(at("2026-11-06T10:00:00Z")) };
        let p = score(&t, at("2026-11-06T09:00:00Z")).unwrap();
        let r = next(&t, &p, at("2026-11-06T09:00:00Z"), 7, &Settings::default(), &london()).unwrap();
        assert_eq!(r.kind, Kind::Chase);
        assert_eq!(r.title, "Time to chase: Chase PO-0419");
        t.status = Status::Done { at: at("2026-11-04T10:00:00Z") };
        assert!(
            between(&t, &p, at("2026-11-04T09:00:00Z"), at("2026-11-10T09:00:00Z"), &Settings::default(), &london())
                .is_empty()
        );
    }
}
