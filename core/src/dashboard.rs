//! The dashboard: what to do now, the day's lanes and the headline numbers,
//! each with a plain-language explanation. Recomputed whenever data changes
//! and as time passes, because scores move with the clock.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::Serialize;

use crate::model::{Status, Task};
use crate::priority::{self, Priority};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Lane {
    Overdue,
    Today,
    Waiting,
    Later,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub task: Task,
    pub score: u8,
    pub why: String,
    pub overdue: bool,
    pub lane: Lane,
}

#[derive(Debug, Clone, Serialize)]
pub struct Figure {
    pub value: usize,
    pub label: &'static str,
    /// What the number means and how it is counted.
    pub explain: &'static str,
    /// "bad" or "warn" when it needs attention, else "".
    pub tone: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub now: Option<Item>,
    /// Ranked, highest score first, without the `now` item.
    pub items: Vec<Item>,
    pub figures: Vec<Figure>,
}

pub fn build(tasks: &[Task], now: Timestamp, tz: &TimeZone) -> Dashboard {
    let end_of_day = now
        .to_zoned(tz.clone())
        .tomorrow()
        .ok()
        .and_then(|z| z.start_of_day().ok())
        .map(|z| z.timestamp())
        .unwrap_or(now);
    let start_of_day = now.to_zoned(tz.clone()).start_of_day().map(|z| z.timestamp()).unwrap_or(now);

    let mut items: Vec<Item> = priority::rank(tasks, now)
        .into_iter()
        .map(|(t, p)| {
            let lane = lane(t, &p, end_of_day, now);
            Item { why: p.why(), score: p.score, overdue: p.overdue, lane, task: t.clone() }
        })
        .collect();

    let count = |l: Lane| items.iter().filter(|i| i.lane == l).count();
    let (overdue, today, waiting) = (count(Lane::Overdue), count(Lane::Today), count(Lane::Waiting));
    let chase_due =
        items.iter().filter(|i| matches!(i.task.status, Status::Waiting { chase: Some(c), .. } if c <= now)).count();
    let done_today = tasks
        .iter()
        .filter(|t| matches!(t.status, Status::Done { at } if at >= start_of_day && at < end_of_day))
        .count();

    // "Now" is the top job you can act on: not one still waiting on someone.
    let pick = items.iter().position(|i| i.lane != Lane::Waiting || i.score >= 60);
    let now_item = pick.map(|i| items.remove(i));

    let figures = vec![
        Figure {
            value: overdue,
            label: "overdue",
            explain: "Jobs whose deadline or appointment time has passed and that are not done.",
            tone: if overdue > 0 { "bad" } else { "" },
        },
        Figure {
            value: today,
            label: "due today",
            explain: "Jobs with a deadline or appointment later today, plus jobs you pinned.",
            tone: "",
        },
        Figure {
            value: chase_due,
            label: "to chase",
            explain: "Jobs waiting on someone else whose chase date has come.",
            tone: if chase_due > 0 { "warn" } else { "" },
        },
        Figure {
            value: waiting,
            label: "waiting on others",
            explain: "Jobs you handed to someone else. Each has a date to chase them.",
            tone: "",
        },
        Figure { value: done_today, label: "done today", explain: "Jobs you marked done since midnight.", tone: "" },
    ];
    Dashboard { now: now_item, items, figures }
}

fn lane(t: &Task, p: &Priority, end_of_day: Timestamp, now: Timestamp) -> Lane {
    if matches!(t.status, Status::Waiting { .. }) {
        return Lane::Waiting;
    }
    match t.deadline() {
        Some(_) if p.overdue => Lane::Overdue,
        Some(d) if d < end_of_day && d >= now => Lane::Today,
        _ if t.pinned => Lane::Today,
        _ => Lane::Later,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn lanes_figures_and_now() {
        let tz = TimeZone::get("Europe/London").unwrap();
        let now = at("2026-11-04T12:00:00Z");
        let made = at("2026-11-01T09:00:00Z");
        let mut late = Task::new("Return call REF-000214", made);
        late.due = Some(at("2026-11-03T16:00:00Z"));
        let mut today = Task::new("Confirm clinic list", made);
        today.due = Some(at("2026-11-04T16:00:00Z"));
        let mut tomorrow = Task::new("Month-end stats", made);
        tomorrow.due = Some(at("2026-11-05T16:00:00Z"));
        let mut wait = Task::new("Supplier quote", made);
        wait.status = Status::Waiting { on: "supplier".into(), chase: Some(at("2026-11-04T09:00:00Z")) };
        let mut done = Task::new("Order toner", made);
        done.status = Status::Done { at: at("2026-11-04T10:00:00Z") };

        let d = build(&[tomorrow, wait, done, today, late], now, &tz);
        let now_item = d.now.unwrap();
        assert_eq!(now_item.task.title, "Return call REF-000214");
        assert_eq!(now_item.lane, Lane::Overdue);
        let lanes: Vec<_> = d.items.iter().map(|i| (i.task.title.as_str(), i.lane)).collect();
        assert!(lanes.contains(&("Confirm clinic list", Lane::Today)));
        assert!(lanes.contains(&("Month-end stats", Lane::Later)));
        assert!(lanes.contains(&("Supplier quote", Lane::Waiting)));
        let fig = |l: &str| d.figures.iter().find(|f| f.label == l).unwrap().value;
        assert_eq!(
            (fig("overdue"), fig("due today"), fig("to chase"), fig("waiting on others"), fig("done today")),
            (1, 1, 1, 1, 1)
        );
        assert!(d.figures.iter().all(|f| !f.explain.is_empty()));
    }

    #[test]
    fn empty_day() {
        let d = build(&[], at("2026-11-04T12:00:00Z"), &TimeZone::UTC);
        assert!(d.now.is_none() && d.items.is_empty());
        assert_eq!(d.figures[0].tone, "");
    }
}
