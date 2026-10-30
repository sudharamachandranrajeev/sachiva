//! The priority score: 0 to 99, with the reasons behind it in plain words.
//!
//! Urgency is the time left before the deadline minus the work still needed
//! ("slack"). With no slack left urgency is full (60); every day of slack
//! halves it. Importance adds or removes points. Tasks waiting on someone
//! score on their chase date instead, at half weight until it arrives.

use jiff::Timestamp;
use serde::Serialize;

use crate::model::{Importance, Status, Task};

const FULL_URGENCY: f64 = 60.0;
const DEFAULT_EFFORT_MIN: u32 = 15;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Priority {
    pub score: u8,
    pub pinned: bool,
    pub overdue: bool,
    /// Most important reason first.
    pub reasons: Vec<String>,
}

impl Priority {
    /// One sentence for the UI: "Due in 2 h, needs about 30 min, marked important."
    pub fn why(&self) -> String {
        let mut s = self.reasons.join(", ");
        if let Some(first) = s.get(0..1) {
            s = first.to_uppercase() + &s[1..];
        }
        s.push('.');
        s
    }
}

/// `None` for finished tasks and counters: they are not ranked.
pub fn score(task: &Task, now: Timestamp) -> Option<Priority> {
    if task.is_done() || task.is_counter() {
        return None;
    }
    let mut reasons = Vec::new();
    if task.pinned {
        reasons.push("pinned by you".to_string());
    }
    let effort = task.effort_min.unwrap_or(DEFAULT_EFFORT_MIN);
    let mut overdue = false;

    let (urgency, weight) = match &task.status {
        Status::Waiting { on, chase } => {
            let who = if on.is_empty() { "someone".to_string() } else { on.clone() };
            match chase {
                Some(c) if *c <= now => {
                    reasons.push(format!("time to chase {who}"));
                    (FULL_URGENCY, 1.0)
                }
                Some(c) => {
                    reasons.push(format!("waiting on {who}, chase in {}", span(secs(now, *c))));
                    (urgency_for(secs(now, *c), 0), 0.5)
                }
                None => {
                    reasons.push(format!("waiting on {who}, no chase date"));
                    (0.0, 0.5)
                }
            }
        }
        _ => match task.deadline() {
            Some(d) => {
                let left = secs(now, d);
                if left < 0 {
                    overdue = true;
                    reasons.push(format!("overdue by {}", span(-left)));
                } else {
                    let what = if task.appointment == Some(d) { "appointment in" } else { "due in" };
                    reasons.push(format!("{what} {}", span(left)));
                    if task.effort_min.is_some() {
                        reasons.push(format!("needs about {}", span(i64::from(effort) * 60)));
                    }
                }
                (urgency_for(left, effort), 1.0)
            }
            None => {
                let days = secs(task.created_at, now) / 86_400;
                if days >= 3 {
                    reasons.push(format!("no deadline, open {days} days"));
                } else {
                    reasons.push("no deadline".to_string());
                }
                ((days as f64).min(15.0), 1.0)
            }
        },
    };

    let importance = match task.importance {
        Importance::High => {
            reasons.push("marked important".to_string());
            25.0
        }
        Importance::Normal => 0.0,
        Importance::Low => {
            reasons.push("marked low importance".to_string());
            -10.0
        }
    };
    let overdue_bonus = if overdue { 10.0 } else { 0.0 };
    let raw = (urgency + overdue_bonus) * weight + importance + 10.0;
    Some(Priority { score: raw.clamp(0.0, 99.0).round() as u8, pinned: task.pinned, overdue, reasons })
}

/// Rank open tasks: pinned first, then by score, then by deadline.
pub fn rank(tasks: &[Task], now: Timestamp) -> Vec<(&Task, Priority)> {
    let mut v: Vec<_> = tasks.iter().filter_map(|t| score(t, now).map(|p| (t, p))).collect();
    v.sort_by(|(a, pa), (b, pb)| {
        pb.pinned
            .cmp(&pa.pinned)
            .then(pb.score.cmp(&pa.score))
            .then(a.deadline().unwrap_or(Timestamp::MAX).cmp(&b.deadline().unwrap_or(Timestamp::MAX)))
    });
    v
}

fn urgency_for(left_secs: i64, effort_min: u32) -> f64 {
    let slack_h = (left_secs - i64::from(effort_min) * 60) as f64 / 3600.0;
    if slack_h <= 0.0 { FULL_URGENCY } else { FULL_URGENCY * 0.5f64.powf(slack_h / 24.0) }
}

fn secs(from: Timestamp, to: Timestamp) -> i64 {
    to.as_second() - from.as_second()
}

/// "45 min", "3 h 10 min", "2 days".
pub fn span(secs: i64) -> String {
    let m = secs.max(0) / 60;
    if m < 60 {
        format!("{} min", m.max(1))
    } else if m < 24 * 60 {
        match m % 60 {
            0 => format!("{} h", m / 60),
            r => format!("{} h {r} min", m / 60),
        }
    } else {
        let d = m / (24 * 60);
        if d == 1 { "1 day".to_string() } else { format!("{d} days") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn task(due: Option<&str>) -> Task {
        let mut t = Task::new("t", at("2026-10-20T09:00:00Z"));
        t.due = due.map(at);
        t
    }

    #[test]
    fn closer_deadline_scores_higher() {
        let now = at("2026-10-29T12:00:00Z");
        let soon = score(&task(Some("2026-10-29T15:00:00Z")), now).unwrap();
        let later = score(&task(Some("2026-11-02T15:00:00Z")), now).unwrap();
        assert!(soon.score > later.score, "{} vs {}", soon.score, later.score);
        assert_eq!(soon.why(), "Due in 3 h.");
    }

    #[test]
    fn effort_eats_slack() {
        let now = at("2026-10-29T12:00:00Z");
        let mut big = task(Some("2026-10-29T15:00:00Z"));
        big.effort_min = Some(180);
        let p = score(&big, now).unwrap();
        assert_eq!(p.score, 70);
        assert_eq!(p.why(), "Due in 3 h, needs about 3 h.");
    }

    #[test]
    fn overdue_and_important() {
        let now = at("2026-10-29T12:00:00Z");
        let mut t = task(Some("2026-10-28T12:00:00Z"));
        t.importance = Importance::High;
        let p = score(&t, now).unwrap();
        assert!(p.overdue);
        assert_eq!(p.score, 99);
        assert_eq!(p.why(), "Overdue by 1 day, marked important.");
    }

    #[test]
    fn waiting_scores_on_chase_date() {
        let now = at("2026-10-29T12:00:00Z");
        let mut t = task(None);
        t.status = Status::Waiting { on: "supplier".into(), chase: Some(at("2026-10-29T11:00:00Z")) };
        assert_eq!(score(&t, now).unwrap().why(), "Time to chase supplier.");
        t.status = Status::Waiting { on: "supplier".into(), chase: Some(at("2026-11-05T11:00:00Z")) };
        let p = score(&t, now).unwrap();
        assert!(p.score < 15, "{}", p.score);
    }

    #[test]
    fn done_is_not_ranked_and_pinned_comes_first() {
        let now = at("2026-10-29T12:00:00Z");
        let mut done = task(Some("2026-10-29T13:00:00Z"));
        done.status = Status::Done { at: now };
        assert!(score(&done, now).is_none());
        let urgent = task(Some("2026-10-29T13:00:00Z"));
        let mut pinned = task(None);
        pinned.pinned = true;
        let all = [urgent, pinned.clone(), done];
        let ranked = rank(&all, now);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].0.id, pinned.id);
    }

    #[test]
    fn spans_read_naturally() {
        assert_eq!(span(30), "1 min");
        assert_eq!(span(45 * 60), "45 min");
        assert_eq!(span(3 * 3600 + 600), "3 h 10 min");
        assert_eq!(span(86_400 * 2 + 5), "2 days");
    }
}
