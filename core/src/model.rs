//! The task model. Times are stored as UTC timestamps; local wall-clock rules
//! (recurrence, working hours) are applied through a time zone at use.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Importance {
    Low,
    #[default]
    Normal,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Status {
    #[default]
    Open,
    /// Waiting on someone else; `chase` is when to follow up.
    Waiting {
        on: String,
        chase: Option<Timestamp>,
    },
    Done {
        at: Timestamp,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

/// How a task comes back once done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Repeat {
    /// Every `every` days on a fixed schedule.
    Daily { every: u16 },
    /// Every `every` weeks on the given days (empty = the due date's weekday).
    Weekly { every: u16, days: Vec<Weekday> },
    /// Every `every` months on day `day` (1-31), clamped to the month's last
    /// day without drifting: a 31st series stays on the 31st where it exists.
    Monthly { every: u16, day: u8 },
    /// `days` days after it was last done ("water every 3 days from when I last did").
    AfterDone { days: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckItem {
    pub text: String,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub notes: String,
    /// When it must be done by.
    pub due: Option<Timestamp>,
    /// A fixed appointment time, if the job is tied to one.
    pub appointment: Option<Timestamp>,
    /// Estimated minutes of work.
    pub effort_min: Option<u32>,
    #[serde(default)]
    pub importance: Importance,
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
    pub pinned: bool,
    /// Free text such as "Site A"; clinic structure arrives in phase 3.
    #[serde(default)]
    pub site: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub checklist: Vec<CheckItem>,
    pub repeat: Option<Repeat>,
    /// No reminders before this time.
    pub snoozed_until: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Task {
    pub fn new(title: impl Into<String>, now: Timestamp) -> Task {
        Task {
            id: uuid::Uuid::now_v7().to_string(),
            title: title.into(),
            notes: String::new(),
            due: None,
            appointment: None,
            effort_min: None,
            importance: Importance::Normal,
            status: Status::Open,
            pinned: false,
            site: String::new(),
            tags: Vec::new(),
            checklist: Vec::new(),
            repeat: None,
            snoozed_until: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_done(&self) -> bool {
        matches!(self.status, Status::Done { .. })
    }

    /// The time that matters most: the appointment if earlier than the deadline.
    pub fn deadline(&self) -> Option<Timestamp> {
        match (self.due, self.appointment) {
            (Some(d), Some(a)) => Some(d.min(a)),
            (d, a) => d.or(a),
        }
    }
}
