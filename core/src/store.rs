//! The encrypted local store (SQLCipher). Every change is written at once and
//! recorded in `history`, so it can be undone; deleting moves a task to the
//! trash first. Backups are encrypted copies made with `sqlcipher_export`.
//! A backup is never opened in place: [`inspect`] and [`restore_copy`] work on
//! a copy, so a file the user picks is never changed.

use std::path::{Path, PathBuf};

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::counts::{self, Count};
use crate::model::{Kind, Span, Task};
use crate::recur;
use crate::waitlist::{Figures, WaitList};

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
    /// A change the rules do not allow; the text is for the user.
    #[error("{0}")]
    Invalid(String),
}

/// What locks a database: the device key (raw, no key derivation), or a
/// passphrase the user chose for an exported file (SQLCipher's own PBKDF2).
#[derive(Clone, Copy)]
pub enum Secret<'a> {
    Key(&'a [u8; 32]),
    Passphrase(&'a str),
}

impl Secret<'_> {
    /// The value for `PRAGMA key` or `ATTACH ... KEY`.
    fn sql(&self) -> String {
        match self {
            Secret::Key(k) => key_literal(k),
            Secret::Passphrase(p) => (*p).to_owned(),
        }
    }
}

/// What a backup holds, shown before restoring it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub schema: i64,
    pub open_jobs: usize,
    pub done_jobs: usize,
    pub trashed: usize,
    /// When a job was last changed, if any.
    pub last_change: Option<Timestamp>,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Schema steps; index + 1 is the version they lead to. Never edit a step
/// that has shipped: add a new one.
const MIGRATIONS: &[&str] = &[
    "
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
",
    "
    CREATE TABLE counts (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        task_id TEXT NOT NULL,
        at INTEGER NOT NULL,
        delta INTEGER NOT NULL,
        note TEXT NOT NULL DEFAULT ''
    );
    CREATE INDEX counts_task_at ON counts(task_id, at);
    CREATE INDEX counts_at ON counts(at);
    CREATE TABLE waitlists (id TEXT PRIMARY KEY, json TEXT NOT NULL, updated_at INTEGER NOT NULL);
    CREATE TABLE figures (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        list_id TEXT NOT NULL,
        span TEXT NOT NULL,
        start TEXT NOT NULL,
        json TEXT NOT NULL,
        saved_at INTEGER NOT NULL
    );
    CREATE INDEX figures_period ON figures(list_id, span, start, seq);
",
];

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
        apply_key(&conn, Secret::Key(key))?;
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
        // Counts go with their task once it leaves the trash for good.
        tx.execute(
            "DELETE FROM counts WHERE task_id IN (SELECT id FROM tasks WHERE deleted_at IS NOT NULL AND deleted_at < ?)",
            [cut],
        )?;
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
    /// the device key for automatic backups, or a passphrase for a file the
    /// user moves to another device.
    pub fn export(&self, path: &Path, secret: Secret) -> Result<()> {
        export_conn(&self.conn, path, secret)
    }

    pub fn summary(&self) -> Result<Summary> {
        summarise(&self.conn)
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
        self.export(&tmp, Secret::Key(key))?;
        std::fs::rename(&tmp, &path)?;
        prune(dir, "sachiva-", keep)?;
        Ok(Some(path))
    }

    /// Backup made just before installing another version, named after the
    /// version running now, so going back can restore it. Kept apart from
    /// the daily ones: the newest `keep` stay.
    pub fn pre_update_backup(
        &self,
        dir: &Path,
        key: &[u8; 32],
        version: &str,
        now: Timestamp,
        keep: usize,
    ) -> Result<PathBuf> {
        if version.is_empty() || !version.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
            return Err(Error::Io(std::io::Error::other(format!("odd version {version:?}"))));
        }
        std::fs::create_dir_all(dir)?;
        let name = format!("{PRE_UPDATE}{}-{version}.db", now.strftime("%Y-%m-%d-%H%M%S"));
        let path = dir.join(&name);
        let tmp = dir.join(format!("{name}.partial"));
        let _ = std::fs::remove_file(&tmp);
        self.export(&tmp, Secret::Key(key))?;
        std::fs::rename(&tmp, &path)?;
        prune(dir, PRE_UPDATE, keep)?;
        Ok(path)
    }

    /// Add (or, with a negative `delta`, take away) on a target or tally.
    /// Entries are never edited or deleted; a correction is a new entry.
    pub fn add_count(&mut self, task_id: &str, delta: i64, note: &str, now: Timestamp) -> Result<Count> {
        let task = self.task(task_id)?;
        if !matches!(task.kind, Kind::Target { .. } | Kind::Tally { .. }) {
            return Err(Error::Invalid("Only targets and tallies keep counts.".into()));
        }
        let tx = self.conn.transaction()?;
        let total: i64 =
            tx.query_row("SELECT coalesce(sum(delta), 0) FROM counts WHERE task_id = ?", [task_id], |r| r.get(0))?;
        counts::check_entry(delta, note, total).map_err(Error::Invalid)?;
        tx.execute(
            "INSERT INTO counts (task_id, at, delta, note) VALUES (?, ?, ?, ?)",
            params![task_id, now.as_second(), delta, note.trim()],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(Count { id, task_id: task_id.into(), at: now, delta, note: note.trim().into() })
    }

    /// Counts with `at >= since`, all tasks, oldest first.
    pub fn counts_since(&self, since: Timestamp) -> Result<Vec<Count>> {
        let mut st =
            self.conn.prepare("SELECT id, task_id, at, delta, note FROM counts WHERE at >= ? ORDER BY at, id")?;
        let rows = st.query_map([since.as_second()], |r| {
            Ok(Count { id: r.get(0)?, task_id: r.get(1)?, at: ts(r.get(2)?), delta: r.get(3)?, note: r.get(4)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every count of one task, oldest first.
    pub fn counts_of(&self, task_id: &str) -> Result<Vec<Count>> {
        let mut st =
            self.conn.prepare("SELECT id, task_id, at, delta, note FROM counts WHERE task_id = ? ORDER BY at, id")?;
        let rows = st.query_map([task_id], |r| {
            Ok(Count { id: r.get(0)?, task_id: r.get(1)?, at: ts(r.get(2)?), delta: r.get(3)?, note: r.get(4)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn waitlists(&self) -> Result<Vec<WaitList>> {
        let mut st = self.conn.prepare("SELECT json FROM waitlists ORDER BY id")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|j| Ok(serde_json::from_str(&j?)?)).collect()
    }

    pub fn waitlist(&self, id: &str) -> Result<WaitList> {
        let j: Option<String> =
            self.conn.query_row("SELECT json FROM waitlists WHERE id = ?", [id], |r| r.get(0)).optional()?;
        Ok(serde_json::from_str(&j.ok_or_else(|| Error::NotFound(id.into()))?)?)
    }

    /// Create or update a list. Lists are archived, never deleted, because
    /// their figures stay in reports. Changing the number of bands is
    /// refused once figures exist, so old figures keep their meaning.
    pub fn save_waitlist(&mut self, list: &WaitList, now: Timestamp) -> Result<()> {
        list.validate().map_err(Error::Invalid)?;
        if let Ok(old) = self.waitlist(&list.id)
            && old.bands != list.bands
        {
            let used: i64 =
                self.conn.query_row("SELECT count(*) FROM figures WHERE list_id = ?", [&list.id], |r| r.get(0))?;
            if used > 0 {
                return Err(Error::Invalid(
                    "This list already has figures, so its bands cannot change. Make a new list for the new bands."
                        .into(),
                ));
            }
        }
        self.conn.execute(
            "INSERT INTO waitlists (id, json, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET json = ?2, updated_at = ?3",
            params![list.id, serde_json::to_string(list)?, now.as_second()],
        )?;
        Ok(())
    }

    /// Save figures for a period. Earlier saves for the same period are kept.
    pub fn save_figures(&mut self, f: &Figures, now: Timestamp) -> Result<()> {
        let list = self.waitlist(&f.list_id)?;
        f.validate(&list).map_err(Error::Invalid)?;
        self.conn.execute(
            "INSERT INTO figures (list_id, span, start, json, saved_at) VALUES (?, ?, ?, ?, ?)",
            params![f.list_id, span_key(f.span), f.start.to_string(), serde_json::to_string(f)?, now.as_second()],
        )?;
        Ok(())
    }

    /// The newest figures for each list and period of `span` from `from`
    /// to `to` (period starts, inclusive), ordered by list then period.
    pub fn figures(&self, span: Span, from: jiff::civil::Date, to: jiff::civil::Date) -> Result<Vec<Figures>> {
        let mut st = self.conn.prepare(
            "SELECT f.json FROM figures f
             JOIN (SELECT max(seq) AS seq FROM figures WHERE span = ?1 AND start >= ?2 AND start <= ?3
                   GROUP BY list_id, start) m ON m.seq = f.seq
             ORDER BY f.list_id, f.start",
        )?;
        let rows =
            st.query_map(params![span_key(span), from.to_string(), to.to_string()], |r| r.get::<_, String>(0))?;
        rows.map(|j| Ok(serde_json::from_str(&j?)?)).collect()
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

/// Name prefix of the backups made before an update.
pub const PRE_UPDATE: &str = "pre-update-";

/// Keep the newest `keep` backups whose names start with `prefix`. Names
/// carry the date first, so name order is age order.
fn prune(dir: &Path, prefix: &str, keep: usize) -> Result<()> {
    let mut all: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(prefix) && n.ends_with(".db")))
        .collect();
    all.sort();
    let excess = all.len().saturating_sub(keep);
    for old in &all[..excess] {
        std::fs::remove_file(old)?;
    }
    Ok(())
}

fn export_conn(conn: &Connection, path: &Path, secret: Secret) -> Result<()> {
    if path.exists() {
        return Err(Error::Io(std::io::Error::new(std::io::ErrorKind::AlreadyExists, path.display().to_string())));
    }
    let p = path.to_str().ok_or_else(|| Error::Io(std::io::Error::other("backup path is not UTF-8")))?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    conn.execute("ATTACH DATABASE ?1 AS backup KEY ?2", params![p, secret.sql()])?;
    let r = conn
        .query_row("SELECT sqlcipher_export('backup')", [], |_| Ok(()))
        .and_then(|_| conn.execute_batch(&format!("PRAGMA backup.user_version = {version}")));
    conn.execute("DETACH DATABASE backup", [])?;
    Ok(r?)
}

fn summarise(conn: &Connection) -> Result<Summary> {
    let schema: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let mut st = conn.prepare("SELECT json, deleted_at FROM tasks")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?)))?;
    let mut sum = Summary { schema, open_jobs: 0, done_jobs: 0, trashed: 0, last_change: None };
    for row in rows {
        let (json, deleted) = row?;
        let task: Task = serde_json::from_str(&json)?;
        sum.last_change = sum.last_change.max(Some(task.updated_at));
        match (deleted, &task.status) {
            (Some(_), _) => sum.trashed += 1,
            (None, crate::model::Status::Done { .. }) => sum.done_jobs += 1,
            (None, _) => sum.open_jobs += 1,
        }
    }
    Ok(sum)
}

/// Open a copy of the backup at `src` (in `work`, removed again) and say what
/// it holds. `src` itself is only read.
pub fn inspect(src: &Path, secret: Secret, work: &Path) -> Result<Summary> {
    let copy = work.join("inspect.db");
    let r = with_copy(src, secret, &copy, summarise);
    let _ = std::fs::remove_file(&copy);
    r
}

/// Turn the backup at `src` into a database locked with the device `key`,
/// written to `dest` (which must not exist). Refuses a backup from a newer
/// Sachiva. `src` itself is only read; the caller swaps `dest` in.
pub fn restore_copy(src: &Path, secret: Secret, dest: &Path, key: &[u8; 32]) -> Result<Summary> {
    let copy = dest.with_extension("source");
    let r = with_copy(src, secret, &copy, |conn| {
        let sum = summarise(conn)?;
        export_conn(conn, dest, Secret::Key(key))?;
        Ok(sum)
    });
    let _ = std::fs::remove_file(&copy);
    if r.is_err() {
        let _ = std::fs::remove_file(dest);
    }
    r
}

fn with_copy<T>(src: &Path, secret: Secret, copy: &Path, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    let _ = std::fs::remove_file(copy);
    std::fs::copy(src, copy)?;
    let conn = Connection::open(copy)?;
    apply_key(&conn, secret)?;
    if conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)).is_err() {
        return Err(Error::WrongKey);
    }
    let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if v > MIGRATIONS.len() as i64 {
        return Err(Error::TooNew(v));
    }
    f(&conn)
}

fn apply_key(conn: &Connection, secret: Secret) -> Result<()> {
    conn.pragma_update(None, "key", secret.sql())?;
    Ok(())
}

/// SQLCipher's raw-key form: x'<64 hex digits>', used as-is with no key derivation.
fn key_literal(key: &[u8; 32]) -> String {
    let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    format!("x'{hex}'")
}

fn span_key(s: Span) -> &'static str {
    match s {
        Span::Day => "day",
        Span::Week => "week",
        Span::Month => "month",
    }
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
    fn counts_add_up_and_never_go_below_zero() {
        let mut s = Store::memory().unwrap();
        let now = at("2026-11-04T09:00:00Z");
        let mut t = Task::new("Calls", now);
        assert!(s.save(&t, now).is_ok());
        assert!(matches!(s.add_count(&t.id, 1, "", now), Err(Error::Invalid(_))), "a plain job keeps no counts");
        t.kind = Kind::Tally { unit: "calls".into() };
        s.save(&t, now).unwrap();
        s.add_count(&t.id, 3, "", now).unwrap();
        s.add_count(&t.id, -1, "miscounted", at("2026-11-04T10:00:00Z")).unwrap();
        assert!(matches!(s.add_count(&t.id, -5, "", now), Err(Error::Invalid(_))));
        let all = s.counts_of(&t.id).unwrap();
        assert_eq!(all.iter().map(|c| c.delta).sum::<i64>(), 2);
        assert_eq!(all[1].note, "miscounted");
        assert_eq!(s.counts_since(at("2026-11-04T09:30:00Z")).unwrap().len(), 1);
        // Undo of task edits does not touch counts.
        s.undo().unwrap();
        assert_eq!(s.counts_of(&t.id).unwrap().len(), 2);
    }

    #[test]
    fn figures_keep_every_save_and_show_the_newest() {
        use jiff::civil::date;
        let mut s = Store::memory().unwrap();
        let now = at("2026-11-04T09:00:00Z");
        let mut l = WaitList::new("Clinic A");
        s.save_waitlist(&l, now).unwrap();
        let mut f = Figures::empty(&l.id, Span::Week, date(2026, 11, 2), 4);
        f.total = Some(120);
        s.save_figures(&f, now).unwrap();
        f.total = Some(118);
        s.save_figures(&f, now).unwrap();
        let mut g = Figures::empty(&l.id, Span::Week, date(2026, 10, 26), 4);
        g.total = Some(125);
        s.save_figures(&g, now).unwrap();
        let got = s.figures(Span::Week, date(2026, 10, 1), date(2026, 11, 30)).unwrap();
        assert_eq!(got.iter().map(|f| f.total).collect::<Vec<_>>(), vec![Some(125), Some(118)]);
        assert!(s.figures(Span::Month, date(2026, 10, 1), date(2026, 11, 30)).unwrap().is_empty());
        let saves: i64 = s.conn.query_row("SELECT count(*) FROM figures", [], |r| r.get(0)).unwrap();
        assert_eq!(saves, 3, "the first save is still there");
        // Bands are fixed once figures exist.
        l.bands = vec![6, 52];
        assert!(matches!(s.save_waitlist(&l, now), Err(Error::Invalid(_))));
        // Bad figures are refused.
        f.start = date(2026, 11, 3);
        assert!(matches!(s.save_figures(&f, now), Err(Error::Invalid(_))));
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
            // A backup locked with another key.
            let other = [9u8; 32];
            let moved = dir.path().join("moved.db");
            s.export(&moved, Secret::Key(&other)).unwrap();
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

    #[test]
    fn passphrase_export_restores_under_the_device_key_and_leaves_the_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let now = at("2026-11-04T09:00:00Z");
        let mut s = Store::open(&dir.path().join("a.db"), &KEY).unwrap();
        s.save(&Task::new("Order gloves", now), now).unwrap();
        let mut done = Task::new("Book interpreter", now);
        done.updated_at = at("2026-11-05T10:00:00Z");
        s.save(&done, now).unwrap();
        s.complete(&done.id, at("2026-11-05T10:00:00Z"), &TimeZone::UTC).unwrap();
        let gone = Task::new("Old note", now);
        s.save(&gone, now).unwrap();
        s.delete(&gone.id, now).unwrap();

        let file = dir.path().join("export.db");
        s.export(&file, Secret::Passphrase("correct horse 'battery'")).unwrap();
        let before = std::fs::read(&file).unwrap();
        assert!(matches!(inspect(&file, Secret::Passphrase("wrong"), dir.path()), Err(Error::WrongKey)));
        assert!(matches!(inspect(&file, Secret::Key(&KEY), dir.path()), Err(Error::WrongKey)));
        let sum = inspect(&file, Secret::Passphrase("correct horse 'battery'"), dir.path()).unwrap();
        assert_eq!((sum.open_jobs, sum.done_jobs, sum.trashed), (1, 1, 1));
        assert_eq!(sum.schema, MIGRATIONS.len() as i64);

        let new_key = [3u8; 32];
        let dest = dir.path().join("restored.db");
        restore_copy(&file, Secret::Passphrase("correct horse 'battery'"), &dest, &new_key).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), before, "the picked file must not change");
        let r = Store::open(&dest, &new_key).unwrap();
        assert_eq!(r.tasks().unwrap().len(), 2);
        assert_eq!(r.trash().unwrap().len(), 1);
        // Leftovers from the work copy are gone.
        assert!(!dest.with_extension("source").exists());
        assert!(!dir.path().join("inspect.db").exists());
    }

    #[test]
    fn a_backup_from_a_newer_sachiva_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.db");
        {
            let s = Store::open(&path, &KEY).unwrap();
            s.conn.pragma_update(None, "user_version", 99).unwrap();
        }
        let dest = dir.path().join("out.db");
        assert!(matches!(restore_copy(&path, Secret::Key(&KEY), &dest, &KEY), Err(Error::TooNew(99))));
        assert!(!dest.exists());
    }

    #[test]
    fn pre_update_backups_are_kept_apart_from_daily_ones() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("a.db"), &KEY).unwrap();
        let b = dir.path().join("b");
        s.daily_backup(&b, &KEY, at("2026-11-01T09:00:00Z"), 1).unwrap();
        for (i, day) in ["2026-11-02", "2026-11-03", "2026-11-04", "2026-11-05"].iter().enumerate() {
            s.pre_update_backup(&b, &KEY, &format!("0.1.{i}"), at(&format!("{day}T09:00:00Z")), 3).unwrap();
        }
        s.daily_backup(&b, &KEY, at("2026-11-06T09:00:00Z"), 1).unwrap();
        let mut names: Vec<_> =
            std::fs::read_dir(&b).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "pre-update-2026-11-03-090000-0.1.1.db",
                "pre-update-2026-11-04-090000-0.1.2.db",
                "pre-update-2026-11-05-090000-0.1.3.db",
                "sachiva-2026-11-06.db"
            ]
        );
        assert!(s.pre_update_backup(&b, &KEY, "../x", at("2026-11-07T09:00:00Z"), 3).is_err());
    }
}
