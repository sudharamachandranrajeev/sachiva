//! The encrypted local store (SQLCipher). Every change is written at once and
//! recorded in `history`, so it can be undone; deleting moves a task to the
//! trash first. Backups are encrypted copies made with `sqlcipher_export`.

use std::path::{Path, PathBuf};

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::model::Task;
use crate::recur;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("file: {0}")]
    Io(#[from] std::io::Error),
    #[error("the key does not open this database")]
    WrongKey,
    #[error("no task with id {0}")]
    NotFound(String),
    #[error("this database was made by a newer Sachiva (schema {0}); update the app")]
    TooNew(i64),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Schema steps; index + 1 is the version they lead to. Never edit a step
/// that has shipped: add a new one.
const MIGRATIONS: &[&str] = &["
    CREATE TABLE tasks (
        id TEXT PRIMARY KEY,
        json TEXT NOT NULL,
        updated_at INTEGER NOT NULL,
        deleted_at INTEGER
    );
    CREATE INDEX tasks_deleted ON tasks(deleted_at);
    CREATE TABLE history (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        group_id INTEGER NOT NULL,
        task_id TEXT NOT NULL,
        before_json TEXT,
        before_deleted_at INTEGER,
        label TEXT NOT NULL,
        at INTEGER NOT NULL
    );
    CREATE TABLE kv (key TEXT PRIMARY KEY, json TEXT NOT NULL);
"];

pub struct Store {
    conn: Connection,
}

/// What `undo` put back, for the "Undone: ..." message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Undone {
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trashed {
    pub task: Task,
    pub deleted_at: Timestamp,
}

impl Store {
    /// Open (or create) the database at `path` with a 32-byte key.
    pub fn open(path: &Path, key: &[u8; 32]) -> Result<Store> {
        let conn = Connection::open(path)?;
        apply_key(&conn, key)?;
        Store::init(conn)
    }

    /// An in-memory store for tests and previews.
    pub fn memory() -> Result<Store> {
        Store::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Store> {
        // A wrong key only shows when the first page is read.
        if conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)).is_err() {
            return Err(Error::WrongKey);
        }
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        let mut store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    fn migrate(&mut self) -> Result<()> {
        let v = self.schema_version()?;
        if v > MIGRATIONS.len() as i64 {
            return Err(Error::TooNew(v));
        }
        for (i, step) in MIGRATIONS.iter().enumerate().skip(v as usize) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(step)?;
            tx.pragma_update(None, "user_version", i as i64 + 1)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn tasks(&self) -> Result<Vec<Task>> {
        let mut st = self.conn.prepare("SELECT json FROM tasks WHERE deleted_at IS NULL ORDER BY id")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|j| Ok(serde_json::from_str(&j?)?)).collect()
    }

    pub fn task(&self, id: &str) -> Result<Task> {
        let j: Option<String> = self
            .conn
            .query_row("SELECT json FROM tasks WHERE id = ? AND deleted_at IS NULL", [id], |r| r.get(0))
            .optional()?;
        Ok(serde_json::from_str(&j.ok_or_else(|| Error::NotFound(id.into()))?)?)
    }

    pub fn trash(&self) -> Result<Vec<Trashed>> {
        let mut st = self
            .conn
            .prepare("SELECT json, deleted_at FROM tasks WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        rows.map(|row| {
            let (j, at) = row?;
            Ok(Trashed { task: serde_json::from_str(&j)?, deleted_at: ts(at) })
        })
        .collect()
    }

    /// Create or update a task.
    pub fn save(&mut self, task: &Task, now: Timestamp) -> Result<()> {
        let label = format!("edit “{}”", task.title);
        self.change(|tx, group| put(tx, group, task, &label, now))
    }

    /// Mark done; a recurring task gets its next copy. Returns the next copy.
    pub fn complete(&mut self, id: &str, now: Timestamp, tz: &TimeZone) -> Result<Option<Task>> {
        let task = self.task(id)?;
        let (done, next) = recur::complete(&task, now, tz);
        let label = format!("finish “{}”", task.title);
        self.change(|tx, group| {
            put(tx, group, &done, &label, now)?;
            if let Some(n) = &next {
                put(tx, group, n, &label, now)?;
            }
            Ok(())
        })?;
        Ok(next)
    }

    /// Move to the trash. It stays restorable until purged.
    pub fn delete(&mut self, id: &str, now: Timestamp) -> Result<()> {
        let task = self.task(id)?;
        let label = format!("delete “{}”", task.title);
        self.change(|tx, group| {
            record(tx, group, id, &label, now)?;
            tx.execute("UPDATE tasks SET deleted_at = ? WHERE id = ?", params![now.as_second(), id])?;
            Ok(())
        })
    }

    pub fn restore(&mut self, id: &str, now: Timestamp) -> Result<()> {
        let label = "restore from trash".to_string();
        self.change(|tx, group| {
            record(tx, group, id, &label, now)?;
            let n = tx.execute("UPDATE tasks SET deleted_at = NULL WHERE id = ? AND deleted_at IS NOT NULL", [id])?;
            if n == 0 { Err(Error::NotFound(id.into())) } else { Ok(()) }
        })
    }

    /// Undo the last change (all rows it touched). `None` when nothing is left.
    pub fn undo(&mut self) -> Result<Option<Undone>> {
        let tx = self.conn.transaction()?;
        let Some((group, label)) = tx
            .query_row("SELECT group_id, label FROM history ORDER BY seq DESC LIMIT 1", [], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })
            .optional()?
        else {
            return Ok(None);
        };
        {
            let mut st = tx.prepare(
                "SELECT task_id, before_json, before_deleted_at FROM history WHERE group_id = ? ORDER BY seq DESC",
            )?;
            let rows: Vec<(String, Option<String>, Option<i64>)> =
                st.query_map([group], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
            for (id, before, deleted) in rows {
                match before {
                    None => {
                        tx.execute("DELETE FROM tasks WHERE id = ?", [&id])?;
                    }
                    Some(j) => {
                        let t: Task = serde_json::from_str(&j)?;
                        tx.execute(
                            "INSERT INTO tasks (id, json, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4)
                             ON CONFLICT(id) DO UPDATE SET json = ?2, updated_at = ?3, deleted_at = ?4",
                            params![id, j, t.updated_at.as_second(), deleted],
                        )?;
                    }
                }
            }
        }
        tx.execute("DELETE FROM history WHERE group_id = ?", [group])?;
        tx.commit()?;
        Ok(Some(Undone { label }))
    }

    /// Remove trash and undo history older than `keep_days`.
    pub fn purge(&mut self, now: Timestamp, keep_days: i64) -> Result<usize> {
        let cut = (now - SignedDuration::from_hours(24 * keep_days)).as_second();
        let tx = self.conn.transaction()?;
        let n = tx.execute("DELETE FROM tasks WHERE deleted_at IS NOT NULL AND deleted_at < ?", [cut])?;
        tx.execute("DELETE FROM history WHERE at < ?", [cut])?;
        tx.commit()?;
        Ok(n)
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let j: Option<String> =
            self.conn.query_row("SELECT json FROM kv WHERE key = ?", [key], |r| r.get(0)).optional()?;
        j.map(|j| Ok(serde_json::from_str(&j)?)).transpose()
    }

    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.conn.execute(
            "INSERT INTO kv (key, json) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET json = ?2",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    /// Write an encrypted copy to `path` (which must not exist), locked with
    /// `key`: the device key for automatic backups, or a passphrase-derived
    /// key for a file the user moves to another device.
    pub fn export(&self, path: &Path, key: &[u8; 32]) -> Result<()> {
        if path.exists() {
            return Err(Error::Io(std::io::Error::new(std::io::ErrorKind::AlreadyExists, path.display().to_string())));
        }
        let p = path.to_str().ok_or_else(|| Error::Io(std::io::Error::other("backup path is not UTF-8")))?;
        self.conn.execute("ATTACH DATABASE ?1 AS backup KEY ?2", params![p, key_literal(key)])?;
        let r = self.conn.query_row("SELECT sqlcipher_export('backup')", [], |_| Ok(())).and_then(|_| {
            self.conn.execute_batch(&format!("PRAGMA backup.user_version = {}", self.schema_version().unwrap_or(0)))
        });
        self.conn.execute("DETACH DATABASE backup", [])?;
        Ok(r?)
    }

    /// Daily backup into `dir`, keeping the newest `keep`. Returns the new
    /// file, or `None` if today's backup already exists.
    pub fn daily_backup(&self, dir: &Path, key: &[u8; 32], now: Timestamp, keep: usize) -> Result<Option<PathBuf>> {
        std::fs::create_dir_all(dir)?;
        let name = format!("sachiva-{}.db", now.strftime("%Y-%m-%d"));
        let path = dir.join(&name);
        if path.exists() {
            return Ok(None);
        }
        let tmp = dir.join(format!("{name}.partial"));
        let _ = std::fs::remove_file(&tmp);
        self.export(&tmp, key)?;
        std::fs::rename(&tmp, &path)?;
        let mut all: Vec<PathBuf> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("sachiva-") && n.ends_with(".db"))
            })
            .collect();
        all.sort();
        let excess = all.len().saturating_sub(keep);
        for old in &all[..excess] {
            std::fs::remove_file(old)?;
        }
        Ok(Some(path))
    }

    /// Run one change in a transaction; all its rows share one undo group.
    fn change(&mut self, f: impl FnOnce(&rusqlite::Transaction, i64) -> Result<()>) -> Result<()> {
        let tx = self.conn.transaction()?;
        let group: i64 = tx.query_row("SELECT coalesce(max(group_id), 0) + 1 FROM history", [], |r| r.get(0))?;
        f(&tx, group)?;
        tx.commit()?;
        Ok(())
    }
}

fn put(tx: &rusqlite::Transaction, group: i64, task: &Task, label: &str, now: Timestamp) -> Result<()> {
    record(tx, group, &task.id, label, now)?;
    tx.execute(
        "INSERT INTO tasks (id, json, updated_at, deleted_at) VALUES (?1, ?2, ?3, NULL)
         ON CONFLICT(id) DO UPDATE SET json = ?2, updated_at = ?3",
        params![task.id, serde_json::to_string(task)?, task.updated_at.as_second()],
    )?;
    Ok(())
}

/// Remember the row as it was before this change.
fn record(tx: &rusqlite::Transaction, group: i64, id: &str, label: &str, now: Timestamp) -> Result<()> {
    let before: Option<(String, Option<i64>)> = tx
        .query_row("SELECT json, deleted_at FROM tasks WHERE id = ?", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let (json, deleted) = before.map_or((None, None), |(j, d)| (Some(j), d));
    tx.execute(
        "INSERT INTO history (group_id, task_id, before_json, before_deleted_at, label, at) VALUES (?, ?, ?, ?, ?, ?)",
        params![group, id, json, deleted, label, now.as_second()],
    )?;
    Ok(())
}

fn apply_key(conn: &Connection, key: &[u8; 32]) -> Result<()> {
    conn.execute_batch(&format!("PRAGMA key = \"{}\";", key_literal(key)))?;
    Ok(())
}

/// SQLCipher's raw-key form: x'<64 hex digits>', used as-is with no key derivation.
fn key_literal(key: &[u8; 32]) -> String {
    let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    format!("x'{hex}'")
}

fn ts(s: i64) -> Timestamp {
    Timestamp::from_second(s).unwrap_or(Timestamp::UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Repeat;

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }
    const KEY: [u8; 32] = [7; 32];

    #[test]
    fn save_edit_undo() {
        let mut s = Store::memory().unwrap();
        let now = at("2026-11-04T09:00:00Z");
        let mut t = Task::new("Order gloves", now);
        s.save(&t, now).unwrap();
        t.title = "Order gloves, size M".into();
        s.save(&t, now).unwrap();
        assert_eq!(s.task(&t.id).unwrap().title, "Order gloves, size M");
        assert_eq!(s.undo().unwrap().unwrap().label, "edit “Order gloves, size M”");
        assert_eq!(s.task(&t.id).unwrap().title, "Order gloves");
        s.undo().unwrap();
        assert!(s.tasks().unwrap().is_empty());
        assert!(s.undo().unwrap().is_none());
    }

    #[test]
    fn delete_goes_to_trash_and_undo_brings_it_back() {
        let mut s = Store::memory().unwrap();
        let now = at("2026-11-04T09:00:00Z");
        let t = Task::new("Scan letters", now);
        s.save(&t, now).unwrap();
        s.delete(&t.id, now).unwrap();
        assert!(s.tasks().unwrap().is_empty());
        assert_eq!(s.trash().unwrap()[0].task.id, t.id);
        s.restore(&t.id, now).unwrap();
        assert_eq!(s.tasks().unwrap().len(), 1);
        s.undo().unwrap();
        s.undo().unwrap();
        assert_eq!(s.tasks().unwrap().len(), 1, "undoing the delete restores it");
        assert!(s.trash().unwrap().is_empty());
        // Purge only removes old trash.
        s.delete(&t.id, now).unwrap();
        assert_eq!(s.purge(now + SignedDuration::from_hours(24), 30).unwrap(), 0);
        assert_eq!(s.purge(now + SignedDuration::from_hours(24 * 31), 30).unwrap(), 1);
    }

    #[test]
    fn completing_a_recurring_task_is_one_undo_step() {
        let mut s = Store::memory().unwrap();
        let now = at("2026-11-04T09:00:00Z");
        let mut t = Task::new("Stock check", now);
        t.due = Some(at("2026-11-04T16:00:00Z"));
        t.repeat = Some(Repeat::Weekly { every: 1, days: vec![] });
        s.save(&t, now).unwrap();
        let next = s.complete(&t.id, now, &TimeZone::get("Europe/London").unwrap()).unwrap().unwrap();
        assert_eq!(s.tasks().unwrap().len(), 2);
        assert!(s.task(&t.id).unwrap().is_done());
        s.undo().unwrap();
        assert_eq!(s.tasks().unwrap().len(), 1);
        assert!(!s.task(&t.id).unwrap().is_done());
        assert!(s.task(&next.id).is_err());
    }

    #[test]
    fn encrypted_file_needs_the_key_and_backups_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sachiva.db");
        let now = at("2026-11-04T09:00:00Z");
        {
            let mut s = Store::open(&path, &KEY).unwrap();
            s.save(&Task::new("Confirm clinic list", now), now).unwrap();
            s.set("theme", &"light").unwrap();
            let b = s.daily_backup(&dir.path().join("backups"), &KEY, now, 2).unwrap().unwrap();
            assert!(s.daily_backup(&dir.path().join("backups"), &KEY, now, 2).unwrap().is_none());
            let copy = Store::open(&b, &KEY).unwrap();
            assert_eq!(copy.tasks().unwrap()[0].title, "Confirm clinic list");
            assert_eq!(copy.schema_version().unwrap(), MIGRATIONS.len() as i64);
            // A backup locked with another key (a passphrase file).
            let other = [9u8; 32];
            let moved = dir.path().join("moved.db");
            s.export(&moved, &other).unwrap();
            assert!(matches!(Store::open(&moved, &KEY), Err(Error::WrongKey)));
            assert_eq!(Store::open(&moved, &other).unwrap().tasks().unwrap().len(), 1);
        }
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.windows(15).any(|w| w == b"SQLite format 3"), "file must be encrypted");
        assert!(!raw.windows(7).any(|w| w == b"Confirm"));
        assert!(matches!(Store::open(&path, &[1; 32]), Err(Error::WrongKey)));
        let s = Store::open(&path, &KEY).unwrap();
        assert_eq!(s.get::<String>("theme").unwrap().as_deref(), Some("light"));
    }

    #[test]
    fn keeps_only_the_newest_backups() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("a.db"), &KEY).unwrap();
        let b = dir.path().join("b");
        for day in ["2026-11-01", "2026-11-02", "2026-11-03"] {
            s.daily_backup(&b, &KEY, at(&format!("{day}T09:00:00Z")), 2).unwrap();
        }
        let mut names: Vec<_> =
            std::fs::read_dir(&b).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(names, ["sachiva-2026-11-02.db", "sachiva-2026-11-03.db"]);
    }
}
