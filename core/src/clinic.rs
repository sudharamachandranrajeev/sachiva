//! Reference-only cancellation work. The booking system remains the record.

use jiff::{
    SignedDuration, Timestamp,
    civil::{Date, Time},
    tz::TimeZone,
};
use serde::{Deserialize, Serialize};

use crate::{
    model::{Importance, MAX_NAME, MAX_TITLE, Task},
    priority, reminders,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum EntryState {
    Waiting,
    Archived,
    Booked { slot_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub list_id: String,
    pub reference: String,
    pub target: Date,
    pub appointment_type: String,
    pub minutes: u16,
    /// Monday = 1, Sunday = 7; empty means any day.
    pub days: Vec<u8>,
    pub earliest: Option<Time>,
    pub latest: Option<Time>,
    pub state: EntryState,
    pub revision: u32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Entry {
    pub fn new(list_id: String, reference: String, target: Date, now: Timestamp) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            list_id,
            reference,
            target,
            appointment_type: String::new(),
            minutes: 30,
            days: vec![],
            earliest: None,
            latest: None,
            state: EntryState::Waiting,
            revision: 0,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        ids(&self.id, &self.list_id)?;
        if self.reference.is_empty()
            || self.reference.len() > 40
            || !self.reference.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_/ .".contains(&b))
            || self.reference.contains(' ')
        {
            return Err("Use a reference code of up to 40 letters, digits, dots, slashes or dashes; no names or contact details.".into());
        }
        details(self.minutes, &self.appointment_type)?;
        if self.days.len() > 7 || self.days.iter().any(|d| !(1..=7).contains(d)) {
            return Err("Pick weekdays from Monday to Sunday.".into());
        }
        if matches!((self.earliest, self.latest), (Some(a), Some(b)) if a >= b) {
            return Err("The preferred end time must follow the start time.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SlotState {
    Open,
    Filled { entry_id: String, at: Timestamp },
    Lost { at: Timestamp },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContactOutcome {
    NoAnswer,
    Declined,
    CallBack,
}

/// A user-entered call result. Its recording time comes from the store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactInput {
    pub entry_id: String,
    pub outcome: ContactOutcome,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub entry_id: String,
    pub at: Timestamp,
    pub outcome: ContactOutcome,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub id: String,
    pub list_id: String,
    pub start: Timestamp,
    pub minutes: u16,
    pub appointment_type: String,
    pub state: SlotState,
    pub contacts: Vec<Contact>,
    pub revision: u32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Slot {
    pub fn new(list_id: String, start: Timestamp, now: Timestamp) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            list_id,
            start,
            minutes: 30,
            appointment_type: String::new(),
            state: SlotState::Open,
            contacts: vec![],
            revision: 0,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        ids(&self.id, &self.list_id)?;
        details(self.minutes, &self.appointment_type)?;
        self.start
            .checked_add(SignedDuration::from_mins(i64::from(self.minutes)))
            .map_err(|_| "The appointment ends outside the supported date range.".to_string())?;
        Ok(())
    }
}

fn ids(id: &str, list: &str) -> Result<(), String> {
    if uuid::Uuid::parse_str(id).is_err() || uuid::Uuid::parse_str(list).is_err() {
        return Err("This clinic record has a broken id.".into());
    }
    Ok(())
}

fn details(minutes: u16, kind: &str) -> Result<(), String> {
    if !(1..=1440).contains(&minutes) {
        return Err("Length must be 1 to 1440 minutes.".into());
    }
    if kind.chars().count() > MAX_NAME {
        return Err(format!("Keep the appointment type under {MAX_NAME} characters."));
    }
    Ok(())
}

/// A hard match: same list (site/clinic/provider), enough time, compatible
/// appointment type and the entire appointment inside the preferred window.
pub fn fits(entry: &Entry, slot: &Slot, tz: &TimeZone) -> bool {
    if entry.list_id != slot.list_id
        || entry.state != EntryState::Waiting
        || entry.minutes > slot.minutes
        || (!entry.appointment_type.is_empty() && !entry.appointment_type.eq_ignore_ascii_case(&slot.appointment_type))
    {
        return false;
    }
    let start = slot.start.to_zoned(tz.clone());
    let Ok(end) = slot.start.checked_add(SignedDuration::from_mins(i64::from(entry.minutes))) else {
        return false;
    };
    let end = end.to_zoned(tz.clone());
    if !entry.days.is_empty() && !entry.days.contains(&(start.weekday().to_monday_one_offset() as u8)) {
        return false;
    }
    if entry.earliest.is_some_and(|t| start.time() < t) {
        return false;
    }
    if entry.latest.is_some_and(|t| end.date() != start.date() || end.time() > t) {
        return false;
    }
    !slot.contacts.iter().any(|c| c.entry_id == entry.id && c.outcome == ContactOutcome::Declined)
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub entry: Entry,
    pub why: String,
}

pub fn candidates(entries: &[Entry], slot: &Slot, now: Timestamp, tz: &TimeZone) -> Vec<Candidate> {
    if slot.state != SlotState::Open || slot.start <= now {
        return vec![];
    }
    let mut out: Vec<_> = entries.iter().filter(|e| fits(e, slot, tz)).cloned().collect();
    out.sort_by(|a, b| a.target.cmp(&b.target).then(a.created_at.cmp(&b.created_at)).then(a.id.cmp(&b.id)));
    out.into_iter()
        .map(|entry| Candidate {
            why: format!(
                "Target {}; same site, clinic and provider; length, type and preferred times fit.",
                entry.target
            ),
            entry,
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub open: usize,
    pub filled: usize,
    pub lost: usize,
    pub refill_percent: Option<f64>,
    pub mean_fill_minutes: Option<f64>,
}

pub fn summary(slots: &[Slot]) -> Summary {
    let mut s = Summary::default();
    let mut seconds = 0i64;
    for slot in slots {
        match slot.state {
            SlotState::Open => s.open += 1,
            SlotState::Lost { .. } => s.lost += 1,
            SlotState::Filled { at, .. } => {
                s.filled += 1;
                seconds += (at.as_second() - slot.created_at.as_second()).max(0);
            }
        }
    }
    if s.filled + s.lost > 0 {
        s.refill_percent = Some(100.0 * s.filled as f64 / (s.filled + s.lost) as f64);
    }
    if s.filled > 0 {
        s.mean_fill_minutes = Some(seconds as f64 / 60.0 / s.filled as f64);
    }
    s
}

/// Reuse the task reminder rules without creating an independently editable
/// task that could be completed while the slot remains open. Stop at slot start.
pub fn reminders(
    slot: &Slot,
    list_name: &str,
    from: Timestamp,
    to: Timestamp,
    settings: &reminders::Settings,
    tz: &TimeZone,
) -> Vec<reminders::Reminder> {
    if slot.state != SlotState::Open || from >= slot.start {
        return vec![];
    }
    let mut task = Task::new(format!("Refill cancelled slot: {list_name}"), slot.created_at);
    task.id = slot.id.clone();
    task.due = Some(slot.start);
    task.effort_min = Some(10);
    task.importance = Importance::High;
    task.updated_at = slot.updated_at;
    let Some(p) = priority::score(&task, from) else {
        return vec![];
    };
    reminders::between(&task, &p, from, to.min(slot.start), settings, tz)
}

pub fn validate_note(note: &str) -> Result<(), String> {
    if note.chars().count() > MAX_TITLE {
        Err(format!("Keep call notes under {MAX_TITLE} characters; reference codes only."))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }
    fn fixture() -> (Entry, Slot) {
        let now = at("2026-11-03T09:00:00Z");
        let list = uuid::Uuid::now_v7().to_string();
        (
            Entry::new(list.clone(), "REF-A17".into(), "2026-11-05".parse().unwrap(), now),
            Slot::new(list, at("2026-11-04T10:30:00Z"), now),
        )
    }
    #[test]
    fn matches_entire_local_window_and_ranks_target_dates() {
        let (mut e, s) = fixture();
        let tz = TimeZone::get("Europe/London").unwrap();
        e.days = vec![3];
        e.earliest = Some("09:00".parse().unwrap());
        e.latest = Some("11:00".parse().unwrap());
        assert!(fits(&e, &s, &tz));
        e.latest = Some("10:59".parse().unwrap());
        assert!(!fits(&e, &s, &tz));
        e.latest = None;
        e.days = vec![4];
        assert!(!fits(&e, &s, &tz));
        e.days.clear();
        e.appointment_type = "Review".into();
        assert!(!fits(&e, &s, &tz));
        e.appointment_type.clear();
        let mut earlier = e.clone();
        earlier.id = uuid::Uuid::now_v7().to_string();
        earlier.target = "2026-11-04".parse().unwrap();
        let ranked = candidates(&[e.clone(), earlier.clone()], &s, s.created_at, &tz);
        assert_eq!(ranked[0].entry.id, earlier.id);
        e.list_id = uuid::Uuid::now_v7().to_string();
        assert!(!fits(&e, &s, &tz));
        e.list_id = s.list_id.clone();
        e.minutes = 31;
        assert!(!fits(&e, &s, &tz));
        assert!(candidates(&[earlier], &s, s.start, &tz).is_empty());
    }
    #[test]
    fn local_preferences_across_clock_change() {
        let (mut e, mut s) = fixture();
        let tz = TimeZone::get("Europe/London").unwrap();
        e.days = vec![7];
        e.earliest = Some("01:00".parse().unwrap());
        e.latest = Some("02:00".parse().unwrap());
        s.start = at("2026-10-25T01:15:00Z");
        assert!(fits(&e, &s, &tz));
        s.start = at("2026-10-25T02:15:00Z");
        assert!(!fits(&e, &s, &tz));
        e.reference = "Patient Name".into();
        assert!(e.validate().is_err());
    }
    #[test]
    fn closed_slots_stop_reminders_and_summary_uses_closed_denominator() {
        let (_, mut slot) = fixture();
        let settings = reminders::Settings { quiet_after_edit_min: 0, ..Default::default() };
        let from = at("2026-11-04T09:00:00Z");
        let to = at("2026-11-04T18:00:00Z");
        let r = reminders(&slot, "Clinic A", from, to, &settings, &TimeZone::UTC);
        assert!(!r.is_empty());
        assert!(r.iter().all(|r| r.at <= slot.start));
        slot.state = SlotState::Filled {
            entry_id: uuid::Uuid::now_v7().to_string(),
            at: slot.created_at + SignedDuration::from_mins(20),
        };
        assert!(reminders(&slot, "Clinic A", from, to, &settings, &TimeZone::UTC).is_empty());
        let mut lost = slot.clone();
        lost.state = SlotState::Lost { at: from };
        let mut open = slot.clone();
        open.state = SlotState::Open;
        let s = summary(&[slot, lost, open]);
        assert_eq!((s.open, s.filled, s.lost), (1, 1, 1));
        assert_eq!(s.refill_percent, Some(50.0));
        assert_eq!(s.mean_fill_minutes, Some(20.0));
    }
}
