//! Atomic calendar import and duplicate tracking, sharing the regular undo log.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarItem {
    pub uid: String,
    pub task: Task,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Imported {
    task_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarResult {
    pub imported: usize,
    pub duplicates: usize,
}

impl Store {
    pub fn import_calendar(&mut self, items: Vec<CalendarItem>, now: Timestamp) -> Result<CalendarResult> {
        if items.is_empty() || items.len() > 100 {
            return Err(Error::Invalid("Choose between 1 and 100 calendar entries.".into()));
        }
        let mut ids = HashSet::new();
        for item in &items {
            if item.uid.trim().is_empty() || item.uid.len() > 1024 || item.uid.chars().any(char::is_control) {
                return Err(Error::Invalid("A calendar entry has a missing or invalid UID.".into()));
            }
            if !ids.insert(item.uid.clone()) {
                return Err(Error::Invalid("The calendar contains duplicate UIDs.".into()));
            }
            item.task.validate().map_err(Error::Invalid)?;
            if !matches!(item.task.status, crate::model::Status::Open)
                || item.task.workflow.is_some()
                || item.task.repeat.is_some()
                || !matches!(item.task.kind, Kind::Appointment | Kind::Deadline)
            {
                return Err(Error::Invalid("A calendar entry has an invalid task type or state.".into()));
            }
        }
        self.change(|tx, group| {
            let mut result = CalendarResult { imported: 0, duplicates: 0 };
            for item in &items {
                let exists: bool =
                    tx.query_row("SELECT EXISTS(SELECT 1 FROM calendar_imports WHERE id=?)", [&item.uid], |r| {
                        r.get(0)
                    })?;
                if exists {
                    result.duplicates += 1;
                    continue;
                }
                let task_exists: bool =
                    tx.query_row("SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?)", [&item.task.id], |r| r.get(0))?;
                if task_exists {
                    return Err(Error::Invalid("An imported job ID already exists. Try the import again.".into()));
                }
                let mut task = item.task.clone();
                task.created_at = now;
                task.updated_at = now;
                put(tx, group, &task, "import calendar", now)?;
                records::put(
                    tx,
                    group,
                    "calendar_import",
                    &item.uid,
                    &Imported { task_id: task.id },
                    "import calendar",
                    now,
                )?;
                result.imported += 1;
            }
            Ok(result)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn now() -> Timestamp {
        "2026-11-02T10:00:00Z".parse().unwrap()
    }
    fn item(uid: &str, title: &str) -> CalendarItem {
        let mut task = Task::new(title, now());
        task.kind = Kind::Appointment;
        CalendarItem { uid: uid.into(), task }
    }
    #[test]
    fn import_is_atomic_deduplicated_and_undoable() {
        let mut s = Store::memory().unwrap();
        let result = s.import_calendar(vec![item("a", "Meeting"), item("b", "Delivery")], now()).unwrap();
        assert_eq!((result.imported, result.duplicates), (2, 0));
        let result = s.import_calendar(vec![item("a", "Meeting"), item("c", "Review")], now()).unwrap();
        assert_eq!((result.imported, result.duplicates), (1, 1));
        assert_eq!(s.tasks().unwrap().len(), 3);
        s.undo().unwrap();
        assert_eq!(s.tasks().unwrap().len(), 2);
        assert_eq!(s.import_calendar(vec![item("c", "Review")], now()).unwrap().imported, 1);
        assert!(s.import_calendar(vec![item("d", "Fine"), item("e", "")], now()).is_err());
        assert_eq!(s.tasks().unwrap().len(), 3);
    }
    #[test]
    fn validation_failure_rolls_back_the_whole_batch() {
        let mut s = Store::memory().unwrap();
        let first = item("one", "First job");
        let mut invalid = item("two", "Second job");
        invalid.task.priority_level = Some(uuid::Uuid::now_v7().to_string());
        assert!(s.import_calendar(vec![first, invalid], now()).is_err());
        assert!(s.tasks().unwrap().is_empty());
        assert_eq!(s.import_calendar(vec![item("one", "First job")], now()).unwrap().imported, 1);
    }

    #[test]
    fn encrypted_backup_preserves_import_deduplication() {
        let dir = tempfile::tempdir().unwrap();
        let key = [42; 32];
        let mut s = Store::open(&dir.path().join("main.db"), &key).unwrap();
        s.import_calendar(vec![item("calendar-id", "Meeting")], now()).unwrap();
        let backup = s.daily_backup(&dir.path().join("backups"), &key, now(), 14).unwrap().unwrap();
        let mut restored = Store::open(&backup, &key).unwrap();
        assert_eq!(restored.import_calendar(vec![item("calendar-id", "Meeting")], now()).unwrap().duplicates, 1);
    }

    #[test]
    fn purge_allows_reimport_and_does_not_restore_mapping() {
        let mut s = Store::memory().unwrap();
        s.import_calendar(vec![item("a", "Meeting")], now()).unwrap();
        let task = s.tasks().unwrap().pop().unwrap();
        s.delete(&task.id, now()).unwrap();
        assert_eq!(s.import_calendar(vec![item("a", "Meeting")], now()).unwrap().duplicates, 1);
        s.purge(now() + SignedDuration::from_hours(31 * 24), 30).unwrap();
        assert_eq!(s.import_calendar(vec![item("a", "Meeting")], now()).unwrap().imported, 1);
    }
}
