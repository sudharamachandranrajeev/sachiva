//! Backups the user can see and act on: list, back up now, export a copy
//! locked with a passphrase, and restore. A restore always backs up the
//! current data first, and never changes the file it restores from.

use std::path::{Path, PathBuf};

use sachiva_core::jiff::Timestamp;
use sachiva_core::store::{self, PRE_UPDATE, Secret, Store, Summary};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::{FsExt, OpenOptions};
use tauri_plugin_sachiva_device::from_hex;

use crate::{App, CommandResult, DB_FILE, err};

const MANUAL: &str = "manual-";
const BEFORE_RESTORE: &str = "before-restore-";
const KEEP_MANUAL: usize = 20;
const KEEP_BEFORE_RESTORE: usize = 5;
pub const KEEP_PRE_UPDATE: usize = 3;
/// Where a file picked for restoring is copied, so it can be read like any other.
const PICKED: &str = "picked.db";
const MIN_PASSPHRASE: usize = 10;
/// A backup waiting to replace the data at the next start.
const STAGED: &str = "restore.staged.db";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    name: String,
    /// daily, manual, preUpdate or beforeRestore
    kind: &'static str,
    bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backups {
    folder: String,
    files: Vec<Backup>,
}

impl App {
    pub fn backup_dir(&self) -> PathBuf {
        self.dir.join("backups")
    }

    /// A backup in the backup folder by name. Names come from the UI, so
    /// anything that is not a plain file name there is refused.
    fn backup_path(&self, name: &str) -> CommandResult<PathBuf> {
        let ok = name.ends_with(".db") && !name.contains(['/', '\\']) && !name.starts_with('.');
        let path = self.backup_dir().join(name);
        if !ok || !path.is_file() {
            return Err("That backup is not in the backup folder.".into());
        }
        Ok(path)
    }

    /// Export the open store to `name` in the backup folder with the device key.
    fn backup_as(&self, prefix: &str, keep: usize) -> CommandResult<String> {
        let inner = self.inner.lock().map_err(err)?;
        let (Some(store), Some(key)) = (inner.store.as_ref(), inner.key.as_ref()) else {
            return Err("Sachiva is locked.".into());
        };
        let dir = self.backup_dir();
        std::fs::create_dir_all(&dir).map_err(err)?;
        let name = format!("{prefix}{}.db", Timestamp::now().strftime("%Y-%m-%d-%H%M%S"));
        store.export(&dir.join(&name), Secret::Key(key)).map_err(err)?;
        prune(&dir, prefix, keep);
        Ok(name)
    }

    /// Replace the database with `restored` (already locked with the device
    /// key). The current data is backed up first; if the new file does not
    /// open, the old one is put back.
    pub fn swap_in(&self, restored: &Path) -> CommandResult<String> {
        let saved = self.backup_as(BEFORE_RESTORE, KEEP_BEFORE_RESTORE)?;
        let mut inner = self.inner.lock().map_err(err)?;
        let key = inner.key.ok_or("Sachiva is locked.")?;
        // Closing the store checkpoints the write-ahead log into the file.
        inner.store = None;
        let r = self.replace_db(restored, |db| Store::open(db, &key).map_err(err));
        match r {
            Ok(store) => {
                inner.store = Some(store);
                Ok(saved)
            }
            Err(e) => {
                inner.store = Some(Store::open(&self.dir.join(DB_FILE), &key).map_err(err)?);
                Err(format!("The backup could not be opened, so nothing was changed ({e})."))
            }
        }
    }

    /// Move `new` into place as the database and check it with `open`. If
    /// that fails, the old file is put back. The store must be closed.
    fn replace_db<T>(&self, new: &Path, open: impl FnOnce(&Path) -> CommandResult<T>) -> CommandResult<T> {
        let db = self.dir.join(DB_FILE);
        let old = self.dir.join("sachiva.db.old");
        for side in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(self.dir.join(format!("{DB_FILE}{side}")));
        }
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&db, &old).map_err(err)?;
        match std::fs::rename(new, &db).map_err(err).and_then(|_| open(&db)) {
            Ok(v) => {
                let _ = std::fs::remove_file(&old);
                Ok(v)
            }
            Err(e) => {
                let _ = std::fs::remove_file(&db);
                std::fs::rename(&old, &db).map_err(err)?;
                Err(e)
            }
        }
    }

    /// Going back a version whose data format is older: the pre-update
    /// backup replaces the data at the next start, before anything opens it
    /// (opening it now would upgrade it again).
    pub fn stage_restore(&self, name: &str) -> CommandResult<()> {
        let src = self.backup_path(name)?;
        let tmp = self.dir.join(format!("{STAGED}.partial"));
        std::fs::copy(&src, &tmp).map_err(err)?;
        std::fs::rename(&tmp, self.dir.join(STAGED)).map_err(err)
    }

    pub fn cancel_staged_restore(&self) {
        let _ = std::fs::remove_file(self.dir.join(STAGED));
    }

    /// At start, before the store opens: put a staged backup in place. The
    /// data it replaces was backed up just before the version change.
    pub fn apply_staged_restore(&self, key: &[u8; 32]) -> Option<String> {
        let staged = self.dir.join(STAGED);
        if !staged.is_file() {
            return None;
        }
        let r = self.replace_db(&staged, |db| Store::open(db, key).map(drop).map_err(err));
        self.cancel_staged_restore();
        r.err()
            .map(|e| format!("The backup from before the update could not be put back ({e}). Your data is unchanged."))
    }
}

fn prune(dir: &Path, prefix: &str, keep: usize) {
    let mut all: Vec<PathBuf> =
        std::fs::read_dir(dir).map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
    all.retain(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(prefix) && n.ends_with(".db")));
    all.sort();
    for old in &all[..all.len().saturating_sub(keep)] {
        let _ = std::fs::remove_file(old);
    }
}

fn kind(name: &str) -> &'static str {
    if name.starts_with(PRE_UPDATE) {
        "preUpdate"
    } else if name.starts_with(MANUAL) {
        "manual"
    } else if name.starts_with(BEFORE_RESTORE) {
        "beforeRestore"
    } else {
        "daily"
    }
}

/// Where a restore reads from, and what unlocks it.
fn source(state: &App, name: Option<&str>, passphrase: Option<&str>) -> CommandResult<(PathBuf, Option<String>)> {
    match name {
        Some(n) => Ok((state.backup_path(n)?, None)),
        None => {
            let p = state.dir.join(PICKED);
            if !p.is_file() {
                return Err("Choose a file first.".into());
            }
            Ok((p, Some(passphrase.unwrap_or_default().to_owned())))
        }
    }
}

/// Try each way of opening the file; the first that works wins.
fn with_secret<T>(state: &App, pass: &Option<String>, f: impl Fn(Secret) -> store::Result<T>) -> CommandResult<T> {
    // The device key for our own backups; for a picked file, the recovery
    // key if the text reads as one, then the text as a passphrase.
    let key = match pass {
        None => Some(state.inner.lock().map_err(err)?.key.ok_or("Sachiva is locked.")?),
        Some(p) => from_hex(p),
    };
    let mut tries: Vec<Secret> = key.iter().map(Secret::Key).collect();
    if let Some(p) = pass {
        tries.push(Secret::Passphrase(p));
    }
    let mut last = String::new();
    for s in tries {
        match f(s) {
            Ok(v) => return Ok(v),
            Err(store::Error::WrongKey) => last = "That passphrase or recovery key does not open this file.".into(),
            Err(store::Error::TooNew(v)) => {
                return Err(format!(
                    "This backup was made by a newer Sachiva (data version {v}). Update Sachiva first."
                ));
            }
            Err(e) => return Err(format!("This file could not be read as a Sachiva backup ({e}).")),
        }
    }
    Err(last)
}

#[tauri::command]
pub fn backups(state: State<App>) -> CommandResult<Backups> {
    let dir = state.backup_dir();
    let mut files: Vec<Backup> = std::fs::read_dir(&dir)
        .map(|r| {
            r.filter_map(|e| {
                let e = e.ok()?;
                let name = e.file_name().into_string().ok()?;
                name.ends_with(".db").then(|| Backup {
                    kind: kind(&name),
                    bytes: e.metadata().map(|m| m.len()).unwrap_or(0),
                    name,
                })
            })
            .collect()
        })
        .unwrap_or_default();
    // Every name carries its date right after the prefix: newest first.
    files.sort_by(|a, b| {
        let date = |n: &str| n.trim_start_matches(|c: char| !c.is_ascii_digit()).to_owned();
        date(&b.name).cmp(&date(&a.name))
    });
    Ok(Backups { folder: dir.display().to_string(), files })
}

/// Back up now, whether or not today's backup exists.
#[tauri::command]
pub fn backup_now(state: State<App>) -> CommandResult<String> {
    state.backup_as(MANUAL, KEEP_MANUAL)
}

/// Save a copy locked with a passphrase, to a place the user picks.
/// Returns false when the user cancelled.
#[tauri::command]
pub async fn export_backup(app: AppHandle, state: State<'_, App>, passphrase: String) -> CommandResult<bool> {
    if passphrase.chars().count() < MIN_PASSPHRASE {
        return Err(format!("Use a passphrase of at least {MIN_PASSPHRASE} characters."));
    }
    let name = format!("sachiva-export-{}.db", Timestamp::now().strftime("%Y-%m-%d"));
    let dialog = app.dialog().file().set_file_name(&name).add_filter("Sachiva backup", &["db"]);
    let Some(target) = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file()).await.map_err(err)?
    else {
        return Ok(false);
    };
    let tmp = state.dir.join("export.tmp");
    let _ = std::fs::remove_file(&tmp);
    state.with(|s| s.export(&tmp, Secret::Passphrase(&passphrase)))?;
    let copied = (|| -> std::io::Result<()> {
        let mut out = app.fs().open(target, OpenOptions::new().write(true).create(true).truncate(true).clone())?;
        std::io::copy(&mut std::fs::File::open(&tmp)?, &mut out)?;
        out.sync_all()
    })();
    let _ = std::fs::remove_file(&tmp);
    copied.map_err(|e| format!("The copy could not be saved there ({e})."))?;
    Ok(true)
}

/// Let the user pick a backup file (an export, or a backup from another
/// device). Returns its file name, or nothing when cancelled.
#[tauri::command]
pub async fn pick_backup(app: AppHandle, state: State<'_, App>) -> CommandResult<Option<String>> {
    let dialog = app.dialog().file().add_filter("Sachiva backup", &["db"]);
    let Some(path) = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file()).await.map_err(err)?
    else {
        return Ok(None);
    };
    let label = path.to_string().rsplit(['/', '\\']).next().unwrap_or("backup").to_owned();
    let bytes = app.fs().read(path).map_err(|e| format!("That file could not be read ({e})."))?;
    std::fs::write(state.dir.join(PICKED), bytes).map_err(err)?;
    Ok(Some(label))
}

/// What a backup holds. `name` is a backup in the folder; without it, the
/// picked file, opened with `passphrase`.
#[tauri::command]
pub fn inspect_backup(state: State<App>, name: Option<String>, passphrase: Option<String>) -> CommandResult<Summary> {
    let (path, pass) = source(&state, name.as_deref(), passphrase.as_deref())?;
    with_secret(&state, &pass, |s| store::inspect(&path, s, &state.dir))
}

/// Restore a backup over the current data. Returns the name of the backup
/// made of the data it replaced.
#[tauri::command]
pub fn restore_backup(
    app: AppHandle,
    state: State<App>,
    name: Option<String>,
    passphrase: Option<String>,
) -> CommandResult<String> {
    let (path, pass) = source(&state, name.as_deref(), passphrase.as_deref())?;
    let key = state.inner.lock().map_err(err)?.key.ok_or("Sachiva is locked.")?;
    let dest = state.dir.join("restore.tmp");
    let _ = std::fs::remove_file(&dest);
    with_secret(&state, &pass, |s| store::restore_copy(&path, s, &dest, &key))?;
    let saved = state.swap_in(&dest)?;
    if name.is_none() {
        let _ = std::fs::remove_file(state.dir.join(PICKED));
    }
    crate::changed(&app);
    Ok(saved)
}
