use super::records;
use super::*;
use crate::work::{Template, Workflow};

impl Store {
    pub fn templates(&self) -> Result<Vec<Template>> {
        records::all(&self.conn, "template")
    }

    pub fn create_template(&mut self, task_id: &str, name: &str, now: Timestamp) -> Result<Template> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > crate::model::MAX_NAME {
            return Err(Error::Invalid("Name the template in 1 to 80 characters.".into()));
        }
        let task = self.task(task_id)?;
        task.validate().map_err(Error::Invalid)?;
        let template =
            Template { id: uuid::Uuid::now_v7().to_string(), name: name.into(), task, archived: false, revision: 1 };
        self.change(|tx, group| records::put(tx, group, "template", &template.id, &template, "create template", now))?;
        Ok(template)
    }

    pub fn archive_template(&mut self, id: &str, revision: u32, archived: bool, now: Timestamp) -> Result<()> {
        self.change(|tx, group| {
            let mut t: Template = records::load(tx, "template", id)?;
            if t.revision != revision {
                return records::stale();
            }
            t.archived = archived;
            t.revision += 1;
            records::put(tx, group, "template", id, &t, "archive template", now)
        })
    }

    pub fn task_from_template(&self, id: &str, now: Timestamp) -> Result<Task> {
        let t: Template = records::load(&self.conn, "template", id)?;
        if t.archived {
            return Err(Error::Invalid("This template is archived.".into()));
        }
        Ok(t.instantiate(now))
    }

    pub fn confirm_delivery(&mut self, id: &str, now: Timestamp) -> Result<()> {
        let mut task = self.task(id)?;
        let Some(Workflow::Delivery { confirmed_at, .. }) = &mut task.workflow else {
            return Err(Error::Invalid("This job is not a delivery.".into()));
        };
        if confirmed_at.is_some() {
            return Err(Error::Invalid("This delivery is already confirmed.".into()));
        }
        *confirmed_at = Some(now);
        task.status = crate::model::Status::Done { at: now };
        task.updated_at = now;
        self.save(&task, now)
    }

    pub fn cancel_session(&mut self, id: &str, now: Timestamp) -> Result<()> {
        let mut task = self.task(id)?;
        let Some(Workflow::Session { cancelled, .. }) = &mut task.workflow else {
            return Err(Error::Invalid("This job is not a clinic session.".into()));
        };
        *cancelled = true;
        task.status = crate::model::Status::Done { at: now };
        task.updated_at = now;
        self.save(&task, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::{self, Stage, WorkflowKind};
    fn now() -> Timestamp {
        "2026-11-04T09:00:00Z".parse().unwrap()
    }
    #[test]
    fn procurement_progress_receipt_and_completion_are_validated_and_undoable() {
        let mut s = Store::memory().unwrap();
        let mut t = work::new_task(WorkflowKind::Procurement, None, now());
        t.title = "Synthetic order".into();
        s.save(&t, now()).unwrap();
        assert!(s.complete(&t.id, now(), &TimeZone::UTC).is_err());
        let Some(Workflow::Procurement { stage, quote_ref, po_ref, quantity, received, .. }) = &mut t.workflow else {
            unreachable!()
        };
        *stage = Stage::Partial;
        *quote_ref = "Q-17".into();
        *po_ref = "PO-17".into();
        *quantity = 10;
        *received = 4;
        s.save(&t, now()).unwrap();
        assert!(s.complete(&t.id, now(), &TimeZone::UTC).is_err(), "partial receipt cannot be completed");
        let mut backwards = t.clone();
        let Some(Workflow::Procurement { stage, received, .. }) = &mut backwards.workflow else { unreachable!() };
        *stage = Stage::Ordered;
        *received = 0;
        assert!(s.save(&backwards, now()).is_err(), "progress cannot silently move backwards");
        let Some(Workflow::Procurement { received, .. }) = &mut t.workflow else { unreachable!() };
        *received = 11;
        assert!(s.save(&t, now()).is_err());
        let Some(Workflow::Procurement { received, .. }) = &mut t.workflow else { unreachable!() };
        *received = 10;
        let Some(Workflow::Procurement { stage, invoice_ref, .. }) = &mut t.workflow else { unreachable!() };
        *stage = Stage::Invoice;
        *invoice_ref = "INV-17".into();
        s.save(&t, now()).unwrap();
        s.complete(&t.id, now(), &TimeZone::UTC).unwrap();
        s.undo().unwrap();
        assert!(!s.task(&t.id).unwrap().is_done());
    }
    #[test]
    fn session_identity_delivery_confirmation_and_template_reset() {
        let mut s = Store::memory().unwrap();
        let l = WaitList::new("Clinic A");
        s.save_waitlist(&l, now()).unwrap();
        let mut session = work::new_task(WorkflowKind::Session, Some(l.id), now());
        session.title = "Clinic session".into();
        session.appointment = Some(now() + SignedDuration::from_hours(1));
        s.save(&session, now()).unwrap();
        s.cancel_session(&session.id, now()).unwrap();
        assert!(s.task(&session.id).unwrap().is_done());
        s.undo().unwrap();
        assert_eq!(s.task(&session.id).unwrap(), session);
        let mut delivery = work::new_task(WorkflowKind::Delivery, None, now());
        delivery.title = "Synthetic delivery".into();
        delivery.site = "Site A".into();
        delivery.appointment = Some(now());
        delivery.checklist.push(crate::model::CheckItem { text: "Check boxes".into(), done: true });
        let Some(Workflow::Delivery { window_end, receiver, .. }) = &mut delivery.workflow else { unreachable!() };
        *window_end = Some(now() + SignedDuration::from_hours(2));
        *receiver = "Stores role".into();
        s.save(&delivery, now()).unwrap();
        assert!(s.complete(&delivery.id, now(), &TimeZone::UTC).is_err());
        s.confirm_delivery(&delivery.id, now()).unwrap();
        assert!(s.task(&delivery.id).unwrap().is_done());
        let template = s.create_template(&delivery.id, "Delivery check", now()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("templates.db");
        s.export(&path, Secret::Key(&[8; 32])).unwrap();
        let backup = Store::open(&path, &[8; 32]).unwrap();
        assert_eq!(backup.summary().unwrap().templates, 1);
        assert_eq!(backup.templates().unwrap()[0].name, "Delivery check");
        let fresh = s.task_from_template(&template.id, now()).unwrap();
        assert_ne!(fresh.id, delivery.id);
        assert!(!fresh.is_done());
        assert!(fresh.appointment.is_none());
        assert!(!fresh.checklist[0].done);
        assert!(matches!(fresh.workflow, Some(Workflow::Delivery { confirmed_at: None, window_end: None, .. })));
        s.archive_template(&template.id, template.revision, true, now()).unwrap();
        assert!(s.task_from_template(&template.id, now()).is_err());
        s.undo().unwrap();
        assert!(s.task_from_template(&template.id, now()).is_ok());
        s.undo().unwrap();
        assert!(s.templates().unwrap().is_empty());
    }
    #[test]
    fn schema_three_clinic_records_and_undo_survive_template_migration() {
        let conn = Connection::open_in_memory().unwrap();
        for step in &MIGRATIONS[..3] {
            conn.execute_batch(step).unwrap();
        }
        conn.pragma_update(None, "user_version", 3).unwrap();
        let list = WaitList::new("Clinic A");
        let entry = crate::clinic::Entry::new(list.id.clone(), "REF-17".into(), "2026-11-05".parse().unwrap(), now());
        let slot = crate::clinic::Slot::new(list.id.clone(), now() + SignedDuration::from_hours(2), now());
        conn.execute(
            "INSERT INTO waitlists VALUES(?,?,?)",
            params![list.id, serde_json::to_string(&list).unwrap(), now().as_second()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO clinic_entries VALUES(?,?,?)",
            params![entry.id, serde_json::to_string(&entry).unwrap(), now().as_second()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO clinic_slots VALUES(?,?,?)",
            params![slot.id, serde_json::to_string(&slot).unwrap(), now().as_second()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO history(group_id,task_id,label,at,entity) VALUES(1,?,'create slot',?,'clinic_slot')",
            params![slot.id, now().as_second()],
        )
        .unwrap();
        let mut s = Store::init(conn).unwrap();
        assert_eq!(s.clinic_entries().unwrap(), vec![entry]);
        assert_eq!(s.clinic_slots().unwrap(), vec![slot]);
        assert!(s.templates().unwrap().is_empty());
        s.undo().unwrap();
        assert!(s.clinic_slots().unwrap().is_empty());
        assert_eq!(s.clinic_entries().unwrap().len(), 1);
    }
    #[test]
    fn expected_delivery_participates_in_priority_and_receipt_removes_that_deadline() {
        let mut t = work::new_task(WorkflowKind::Procurement, None, now());
        t.title = "Synthetic expected delivery".into();
        t.due = Some(now() + SignedDuration::from_hours(4));
        let expected = now() + SignedDuration::from_hours(2);
        let Some(Workflow::Procurement { stage, quote_ref, po_ref, expected: e, .. }) = &mut t.workflow else {
            unreachable!()
        };
        *stage = Stage::Ordered;
        *quote_ref = "Q".into();
        *po_ref = "PO".into();
        *e = Some(expected);
        assert_eq!(t.deadline(), Some(expected));
        let Some(Workflow::Procurement { stage, received, .. }) = &mut t.workflow else { unreachable!() };
        *stage = Stage::Received;
        *received = 1;
        assert_eq!(t.deadline(), t.due);
        assert!(t.validate().is_ok());
    }
}
