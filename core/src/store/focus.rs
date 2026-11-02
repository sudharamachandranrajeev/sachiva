//! Task-linked focus sessions, stored with the same transactional undo as tasks.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusSession {
    pub id: String,
    pub task_id: String,
    pub title: String,
    pub started_at: Timestamp,
    pub ended_at: Option<Timestamp>,
}

impl Store {
    pub fn focus_sessions(&self) -> Result<Vec<FocusSession>> {
        let mut sessions: Vec<FocusSession> = records::all(&self.conn, "focus")?;
        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at).then_with(|| b.id.cmp(&a.id)));
        Ok(sessions)
    }

    pub fn start_focus(&mut self, task_id: &str, now: Timestamp) -> Result<FocusSession> {
        self.change(|tx, group| {
            let json: Option<String> = tx
                .query_row("SELECT json FROM tasks WHERE id = ? AND deleted_at IS NULL", [task_id], |r| r.get(0))
                .optional()?;
            let task: Task = serde_json::from_str(&json.ok_or_else(|| Error::NotFound(task_id.into()))?)?;
            if task.is_done() {
                return Err(Error::Invalid("Choose an open job to focus on.".into()));
            }
            let active: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM focus_sessions WHERE json_extract(json, '$.endedAt') IS NULL)",
                [],
                |r| r.get(0),
            )?;
            if active {
                return Err(Error::Invalid("Stop the running focus session first.".into()));
            }
            let session = FocusSession {
                id: uuid::Uuid::now_v7().to_string(),
                task_id: task.id,
                title: task.title,
                started_at: now,
                ended_at: None,
            };
            records::put(tx, group, "focus", &session.id, &session, "start focus", now)?;
            Ok(session)
        })
    }

    pub fn stop_focus(&mut self, id: &str, now: Timestamp) -> Result<FocusSession> {
        self.change(|tx, group| {
            let mut session: FocusSession = records::load(tx, "focus", id)?;
            if session.ended_at.is_some() {
                return Err(Error::Invalid("This focus session has already stopped.".into()));
            }
            if now < session.started_at {
                return Err(Error::Invalid(
                    "The clock is earlier than the start. Correct the clock before stopping.".into(),
                ));
            }
            session.ended_at = Some(now);
            records::put(tx, group, "focus", id, &session, "stop focus", now)?;
            Ok(session)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn one_active_session_persists_and_is_undoable() {
        let mut s = Store::memory().unwrap();
        let now = at("2026-11-02T10:00:00Z");
        let task = Task::new("Prepare report", now);
        s.save(&task, now).unwrap();
        let first = s.start_focus(&task.id, now).unwrap();
        assert!(s.start_focus(&task.id, now).is_err());
        assert!(s.stop_focus(&first.id, at("2026-11-02T09:59:00Z")).is_err());
        let end = at("2026-11-02T10:25:00Z");
        assert_eq!(s.stop_focus(&first.id, end).unwrap().ended_at, Some(end));
        assert!(s.stop_focus(&first.id, end).is_err());
        s.undo().unwrap();
        assert!(s.focus_sessions().unwrap()[0].ended_at.is_none());
        s.undo().unwrap();
        assert!(s.focus_sessions().unwrap().is_empty());
    }

    #[test]
    fn purging_a_trashed_task_removes_its_focus_history() {
        let mut s = Store::memory().unwrap();
        let now = at("2026-11-02T10:00:00Z");
        let task = Task::new("Discarded job", now);
        s.save(&task, now).unwrap();
        let focus = s.start_focus(&task.id, now).unwrap();
        s.stop_focus(&focus.id, now).unwrap();
        s.delete(&task.id, now).unwrap();
        let later = now + SignedDuration::from_hours(24 * 31);
        assert_eq!(s.purge(later, 30).unwrap(), 1);
        assert!(s.focus_sessions().unwrap().is_empty());
        assert!(s.undo().unwrap().is_none());
    }

    #[test]
    fn migration_from_version_four_keeps_tasks_and_history() {
        let conn = Connection::open_in_memory().unwrap();
        for (i, step) in MIGRATIONS.iter().take(4).enumerate() {
            conn.execute_batch(step).unwrap();
            conn.pragma_update(None, "user_version", i as i64 + 1).unwrap();
        }
        let now = at("2026-11-02T10:00:00Z");
        let task = Task::new("Existing job", now);
        conn.execute(
            "INSERT INTO tasks (id,json,updated_at) VALUES (?,?,?)",
            params![task.id, serde_json::to_string(&task).unwrap(), now.as_second()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO history (group_id, task_id, label, at) VALUES (1, ?, 'old change', ?)",
            params![task.id, now.as_second()],
        )
        .unwrap();
        let mut store = Store::init(conn).unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len() as i64);
        assert_eq!(store.task(&task.id).unwrap().title, "Existing job");
        assert!(store.focus_sessions().unwrap().is_empty());
        assert_eq!(store.undo().unwrap().unwrap().label, "old change");
    }

    #[test]
    fn encrypted_backup_preserves_active_session() {
        let dir = tempfile::tempdir().unwrap();
        let key = [42; 32];
        let now = at("2026-11-02T10:00:00Z");
        let mut s = Store::open(&dir.path().join("main.db"), &key).unwrap();
        let task = Task::new("Order supplies", now);
        s.save(&task, now).unwrap();
        let focus = s.start_focus(&task.id, now).unwrap();
        let backup = s.daily_backup(&dir.path().join("backups"), &key, now, 14).unwrap().unwrap();
        let summary = inspect(&backup, Secret::Key(&key), dir.path()).unwrap();
        assert_eq!(summary.focus_sessions, 1);
        let restored = Store::open(&backup, &key).unwrap();
        assert_eq!(restored.focus_sessions().unwrap()[0].id, focus.id);
        assert!(restored.focus_sessions().unwrap()[0].ended_at.is_none());
    }
}
