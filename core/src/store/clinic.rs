//! Clinic writes share the store's transaction and undo history. Filling a
//! slot and booking its reference are one change, never two independent saves.

use super::*;
use crate::clinic::{self, Contact, ContactInput, Entry, EntryState, Slot, SlotState};

use super::records::{all, current_revision, load, put, stale};

fn open_slot(conn: &Connection, id: &str, revision: u32) -> Result<Slot> {
    let s: Slot = load(conn, "clinic_slot", id)?;
    if s.revision != revision {
        return stale();
    }
    if s.state != SlotState::Open {
        return Err(Error::Invalid("This slot is already closed; undo its outcome first.".into()));
    }
    Ok(s)
}

impl Store {
    pub fn clinic_entries(&self) -> Result<Vec<Entry>> {
        all(&self.conn, "clinic_entry")
    }
    pub fn clinic_slots(&self) -> Result<Vec<Slot>> {
        all(&self.conn, "clinic_slot")
    }

    pub fn save_clinic_entry(&mut self, mut entry: Entry, now: Timestamp) -> Result<Entry> {
        entry.reference = entry.reference.trim().into();
        entry.appointment_type = entry.appointment_type.trim().into();
        entry.days.sort_unstable();
        entry.days.dedup();
        entry.validate().map_err(Error::Invalid)?;
        let list = self.waitlist(&entry.list_id)?;
        self.change(|tx, group| {
            let old = current_revision::<Entry>(tx, "clinic_entry", &entry.id, entry.revision, |e| e.revision)?;
            if matches!(entry.state, EntryState::Booked { .. })
                || old.as_ref().is_some_and(|e| matches!(e.state, EntryState::Booked { .. }))
            {
                return Err(Error::Invalid(
                    "A booked reference changes only through its slot outcome and Undo.".into(),
                ));
            }
            if list.archived && old.as_ref().is_none_or(|e| e.list_id != entry.list_id) {
                return Err(Error::Invalid("Choose a list that is not archived.".into()));
            }
            if old.as_ref().is_some_and(|e| e.list_id != entry.list_id || e.reference != entry.reference) {
                return Err(Error::Invalid(
                    "An existing reference cannot change its code or list. Archive it and add a new reference.".into(),
                ));
            }
            let duplicate = all::<Entry>(tx, "clinic_entry")?.iter().any(|e| {
                e.id != entry.id
                    && e.list_id == entry.list_id
                    && e.state == EntryState::Waiting
                    && entry.state == EntryState::Waiting
                    && e.reference.eq_ignore_ascii_case(&entry.reference)
            });
            if duplicate {
                return Err(Error::Invalid("That reference is already waiting on this list.".into()));
            }
            entry.created_at = old.map_or(now, |e| e.created_at);
            entry.updated_at = now;
            entry.revision += 1;
            put(tx, group, "clinic_entry", &entry.id, &entry, "save waiting reference", now)
        })?;
        Ok(entry)
    }

    pub fn save_clinic_slot(&mut self, mut slot: Slot, now: Timestamp) -> Result<Slot> {
        slot.appointment_type = slot.appointment_type.trim().into();
        slot.validate().map_err(Error::Invalid)?;
        let list = self.waitlist(&slot.list_id)?;
        self.change(|tx, group| {
            let old = current_revision::<Slot>(tx, "clinic_slot", &slot.id, slot.revision, |s| s.revision)?;
            if slot.state != SlotState::Open || old.as_ref().is_some_and(|s| s.state != SlotState::Open) {
                return Err(Error::Invalid("Closed slots change through Undo.".into()));
            }
            if list.archived && old.as_ref().is_none_or(|s| s.list_id != slot.list_id) {
                return Err(Error::Invalid("Choose a list that is not archived.".into()));
            }
            if old.as_ref().is_some_and(|s| {
                !s.contacts.is_empty()
                    && (s.list_id != slot.list_id
                        || s.start != slot.start
                        || s.minutes != slot.minutes
                        || s.appointment_type != slot.appointment_type)
            }) {
                return Err(Error::Invalid(
                    "This slot has call history. Undo the calls before changing its details.".into(),
                ));
            }
            slot.contacts = old.as_ref().map_or_else(Vec::new, |s| s.contacts.clone());
            slot.created_at = old.map_or(now, |s| s.created_at);
            slot.updated_at = now;
            slot.revision += 1;
            put(tx, group, "clinic_slot", &slot.id, &slot, "save cancelled slot", now)
        })?;
        Ok(slot)
    }

    pub fn contact_clinic_entry(
        &mut self,
        slot_id: &str,
        revision: u32,
        input: ContactInput,
        now: Timestamp,
        tz: &TimeZone,
    ) -> Result<()> {
        clinic::validate_note(&input.note).map_err(Error::Invalid)?;
        self.change(|tx, group| {
            let mut slot = open_slot(tx, slot_id, revision)?;
            let entry: Entry = load(tx, "clinic_entry", &input.entry_id)?;
            if slot.start <= now || !clinic::fits(&entry, &slot, tz) {
                return Err(Error::Invalid("This reference no longer fits this open slot.".into()));
            }
            if slot.contacts.len() >= crate::model::MAX_ITEMS {
                return Err(Error::Invalid("This slot has reached the call-history limit.".into()));
            }
            slot.contacts.push(Contact {
                entry_id: input.entry_id,
                at: now,
                outcome: input.outcome,
                note: input.note.trim().into(),
            });
            slot.revision += 1;
            // Logging a call must not restart the quiet-after-edit reminder period.
            put(tx, group, "clinic_slot", slot_id, &slot, "log call attempt", now)
        })
    }

    pub fn close_clinic_slot(
        &mut self,
        slot_id: &str,
        revision: u32,
        entry_id: Option<&str>,
        now: Timestamp,
        tz: &TimeZone,
    ) -> Result<()> {
        self.change(|tx, group| {
            let mut slot = open_slot(tx, slot_id, revision)?;
            let label = if entry_id.is_some() { "fill cancelled slot" } else { "mark cancelled slot lost" };
            slot.state = if let Some(id) = entry_id {
                let mut entry: Entry = load(tx, "clinic_entry", id)?;
                if slot.start <= now || !clinic::fits(&entry, &slot, tz) {
                    return Err(Error::Invalid("This reference no longer fits this open slot.".into()));
                }
                entry.state = EntryState::Booked { slot_id: slot_id.into() };
                entry.updated_at = now;
                entry.revision += 1;
                put(tx, group, "clinic_entry", id, &entry, label, now)?;
                SlotState::Filled { entry_id: id.into(), at: now }
            } else {
                SlotState::Lost { at: now }
            };
            slot.updated_at = now;
            slot.revision += 1;
            put(tx, group, "clinic_slot", slot_id, &slot, label, now)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clinic::ContactOutcome;
    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }
    fn setup() -> (Store, Entry, Slot, Timestamp) {
        let now = at("2026-11-04T09:00:00Z");
        let mut s = Store::memory().unwrap();
        let l = WaitList::new("Clinic A");
        s.save_waitlist(&l, now).unwrap();
        let e = s
            .save_clinic_entry(Entry::new(l.id.clone(), "REF-A17".into(), "2026-11-05".parse().unwrap(), now), now)
            .unwrap();
        let slot = s.save_clinic_slot(Slot::new(l.id, now + SignedDuration::from_hours(2), now), now).unwrap();
        (s, e, slot, now)
    }
    #[test]
    fn filling_is_atomic_undoable_and_cannot_book_the_same_reference_twice() {
        let (mut s, e, slot, now) = setup();
        let other = s.save_clinic_slot(Slot::new(slot.list_id.clone(), slot.start, now), now).unwrap();
        s.close_clinic_slot(&slot.id, slot.revision, Some(&e.id), now, &TimeZone::UTC).unwrap();
        assert!(matches!(s.clinic_entries().unwrap()[0].state, EntryState::Booked { .. }));
        assert!(s.close_clinic_slot(&other.id, other.revision, Some(&e.id), now, &TimeZone::UTC).is_err());
        assert!(s.save_clinic_entry(e.clone(), now).is_err(), "stale editor cannot overwrite booking");
        s.undo().unwrap();
        assert_eq!(s.clinic_entries().unwrap()[0].state, EntryState::Waiting);
        assert!(s.clinic_slots().unwrap().iter().all(|s| s.state == SlotState::Open));
        s.close_clinic_slot(&other.id, other.revision, Some(&e.id), now, &TimeZone::UTC).unwrap();
    }
    #[test]
    fn calls_declines_and_undo_preserve_history_and_reminder_floor() {
        let (mut s, e, slot, now) = setup();
        s.contact_clinic_entry(
            &slot.id,
            slot.revision,
            ContactInput { entry_id: e.id.clone(), outcome: ContactOutcome::Declined, note: "Synthetic test".into() },
            now + SignedDuration::from_mins(15),
            &TimeZone::UTC,
        )
        .unwrap();
        let changed = s.clinic_slots().unwrap().remove(0);
        assert_eq!(changed.updated_at, slot.updated_at);
        assert_eq!(s.summary().unwrap().last_change, Some(now + SignedDuration::from_mins(15)));
        assert!(!clinic::fits(&e, &changed, &TimeZone::UTC));
        assert!(
            s.close_clinic_slot(&slot.id, slot.revision, None, now, &TimeZone::UTC).is_err(),
            "stale slot action rejected"
        );
        assert!(
            s.close_clinic_slot(&slot.id, changed.revision, Some(&e.id), now, &TimeZone::UTC).is_err(),
            "declined reference cannot be filled"
        );
        s.undo().unwrap();
        assert!(s.clinic_slots().unwrap()[0].contacts.is_empty());
        s.close_clinic_slot(&slot.id, slot.revision, None, now, &TimeZone::UTC).unwrap();
        s.undo().unwrap();
        assert_eq!(s.clinic_slots().unwrap()[0].state, SlotState::Open);
        s.undo().unwrap();
        assert!(s.clinic_slots().unwrap().is_empty(), "creation is undoable too");
    }
    #[test]
    fn validates_references_list_identity_and_past_slots() {
        let (mut s, e, slot, now) = setup();
        let duplicate = Entry::new(e.list_id.clone(), e.reference.to_lowercase(), e.target, now);
        assert!(s.save_clinic_entry(duplicate, now).is_err());
        let mut renamed = e.clone();
        renamed.reference = "REF-B18".into();
        assert!(s.save_clinic_entry(renamed, now).is_err(), "a stale candidate must keep the same reference identity");
        let mut l = s.waitlist(&e.list_id).unwrap();
        l.provider = "AB".into();
        assert!(s.save_waitlist(&l, now).is_err());
        assert!(s.close_clinic_slot(&slot.id, slot.revision, Some(&e.id), slot.start, &TimeZone::UTC).is_err());
        assert_eq!(s.clinic_entries().unwrap()[0].state, EntryState::Waiting);
        assert_eq!(s.summary().unwrap().clinic_slots, 1);
    }
    #[test]
    fn migration_keeps_v2_tasks_lists_and_undo() {
        let conn = Connection::open_in_memory().unwrap();
        for step in &MIGRATIONS[..2] {
            conn.execute_batch(step).unwrap();
        }
        conn.pragma_update(None, "user_version", 2).unwrap();
        let now = at("2026-11-04T09:00:00Z");
        let t = Task::new("Synthetic old job", now);
        conn.execute(
            "INSERT INTO tasks(id,json,updated_at) VALUES (?,?,?)",
            params![t.id, serde_json::to_string(&t).unwrap(), now.as_second()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO history(group_id,task_id,label,at) VALUES(1,?,'create old task',?)",
            params![t.id, now.as_second()],
        )
        .unwrap();
        let list = WaitList::new("Synthetic old list");
        conn.execute(
            "INSERT INTO waitlists(id,json,updated_at) VALUES(?,?,?)",
            params![list.id, serde_json::to_string(&list).unwrap(), now.as_second()],
        )
        .unwrap();
        let mut figures = Figures::empty(&list.id, Span::Day, "2026-11-04".parse().unwrap(), 4);
        figures.total = Some(7);
        conn.execute(
            "INSERT INTO figures(list_id,span,start,json,saved_at) VALUES(?,'day','2026-11-04',?,?)",
            params![list.id, serde_json::to_string(&figures).unwrap(), now.as_second()],
        )
        .unwrap();
        let mut s = Store::init(conn).unwrap();
        assert_eq!(s.schema_version().unwrap(), MIGRATIONS.len() as i64);
        assert_eq!(s.tasks().unwrap()[0], t);
        assert_eq!(s.waitlists().unwrap(), vec![list]);
        assert_eq!(s.figures(Span::Day, figures.start, figures.start).unwrap(), vec![figures]);
        s.undo().unwrap();
        assert!(s.tasks().unwrap().is_empty());
    }
    #[test]
    fn encrypted_backup_round_trip_keeps_clinic_state() {
        let (mut s, e, slot, now) = setup();
        s.close_clinic_slot(&slot.id, slot.revision, Some(&e.id), now, &TimeZone::UTC).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clinic.db");
        s.export(&path, Secret::Key(&[9; 32])).unwrap();
        let restored = Store::open(&path, &[9; 32]).unwrap();
        assert_eq!(restored.clinic_entries().unwrap(), s.clinic_entries().unwrap());
        assert_eq!(restored.clinic_slots().unwrap(), s.clinic_slots().unwrap());
    }
}
