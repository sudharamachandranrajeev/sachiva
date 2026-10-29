//! Updates and going back a version. Sachiva checks at start and always asks
//! first. Every install, forward or back, makes a backup of the data before
//! it, and every download is checked: on the desktop against the updater
//! signature (the key is in tauri.conf.json), on Android against the
//! release's SHA256SUMS.txt and by Android's own signature check.

use sachiva_core::jiff::Timestamp;
use sachiva_core::store::PRE_UPDATE;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::data::KEEP_PRE_UPDATE;
use crate::{App, CommandResult, err};

const REPO: &str = "https://github.com/sudharamachandranrajeev/sachiva";

/// How this copy of Sachiva was installed, which decides how it updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallKind {
    /// Windows installer (NSIS).
    Installer,
    AppImage,
    /// Arch Linux package, installed with pacman.
    Pacman,
    /// Windows portable zip: cannot update itself.
    Portable,
    #[cfg_attr(desktop, allow(dead_code))]
    Android,
    /// Built from source, or a package Sachiva does not know how to update.
    Other,
}

impl InstallKind {
    fn can_install(self) -> bool {
        !matches!(self, InstallKind::Portable | InstallKind::Other)
    }
}

pub fn install_kind() -> InstallKind {
    #[cfg(target_os = "android")]
    return InstallKind::Android;
    #[cfg(not(target_os = "android"))]
    {
        use tauri::utils::config::BundleType;
        use tauri::utils::platform::bundle_type;
        let exe = std::env::current_exe().ok();
        if cfg!(windows) {
            // The portable zip ships a file named "portable" next to the program.
            if exe.as_ref().and_then(|e| e.parent()).is_some_and(|d| d.join("portable").is_file()) {
                return InstallKind::Portable;
            }
            if bundle_type() == Some(BundleType::Nsis) {
                return InstallKind::Installer;
            }
        }
        if cfg!(target_os = "linux") {
            // The Arch package marks itself: its program is the same file the
            // other Linux bundles start from, so it cannot tell by itself.
            let marked = std::fs::read_to_string("/usr/lib/sachiva/install-type").is_ok_and(|t| t.trim() == "pacman");
            if marked && exe.as_deref() == Some(std::path::Path::new("/usr/bin/sachiva")) {
                return InstallKind::Pacman;
            }
            if std::env::var_os("APPIMAGE").is_some() && bundle_type() == Some(BundleType::AppImage) {
                return InstallKind::AppImage;
            }
        }
        InstallKind::Other
    }
}

/// "ask": check at start and ask before installing. "off": never check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Ask,
    Off,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    kind: InstallKind,
    can_install: bool,
    current: String,
    /// The version before the last update, which "go back" returns to.
    previous: Option<String>,
    mode: Mode,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    version: String,
    notes: Option<String>,
    date: Option<String>,
    can_install: bool,
}

/// What going back would do, shown before the user confirms.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackPlan {
    version: String,
    /// The backup made just before updating from that version, if kept.
    backup: Option<String>,
    /// That version cannot read the data as it is now, so the backup must
    /// replace it: changes made since the update are then lost.
    needs_restore: bool,
}

fn current(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

fn previous(state: &App) -> Option<String> {
    state.with(|s| s.get::<String>("update.previous")).ok().flatten()
}

/// `0.1.10` > `0.1.9`. Anything that is not three numbers counts as 0.0.0.
#[cfg_attr(desktop, allow(dead_code))]
fn newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> (u64, u64, u64) {
        let n: Vec<u64> = v.trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect();
        (n.first().copied().unwrap_or(0), n.get(1).copied().unwrap_or(0), n.get(2).copied().unwrap_or(0))
    };
    parse(a) > parse(b)
}

fn plain_version(v: &str) -> CommandResult<&str> {
    if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.') {
        Ok(v)
    } else {
        Err(format!("{v:?} is not a version number."))
    }
}

/// The newest pre-update backup made while `version` was running.
fn pre_update_backup_of(state: &App, version: &str) -> Option<String> {
    let tail = format!("-{version}.db");
    let mut names: Vec<String> = std::fs::read_dir(state.backup_dir())
        .ok()?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with(PRE_UPDATE) && n.ends_with(&tail))
        .collect();
    names.sort();
    names.pop()
}

/// Back up before installing, and remember which version to go back to.
fn before_install(app: &AppHandle, state: &App) -> CommandResult<()> {
    let version = current(app);
    let inner = state.inner.lock().map_err(err)?;
    let (Some(store), Some(key)) = (inner.store.as_ref(), inner.key.as_ref()) else {
        return Err("Sachiva is locked.".into());
    };
    store
        .pre_update_backup(&state.backup_dir(), key, &version, Timestamp::now(), KEEP_PRE_UPDATE)
        .map_err(|e| format!("The backup before updating failed, so nothing was installed ({e})."))?;
    store.set("update.previous", &version).map_err(err)
}

#[tauri::command]
pub fn update_status(app: AppHandle, state: State<App>) -> UpdateStatus {
    let kind = install_kind();
    UpdateStatus {
        kind,
        can_install: kind.can_install(),
        current: current(&app),
        previous: previous(&state),
        mode: state.with(|s| s.get("updates.mode")).ok().flatten().unwrap_or_default(),
    }
}

#[tauri::command]
pub fn set_update_mode(state: State<App>, mode: Mode) -> CommandResult<()> {
    state.with(|s| s.set("updates.mode", &mode))
}

/// Open one of Sachiva's own pages: the download page, or a new issue
/// prefilled with `fields` (only the known ones are used).
#[tauri::command]
pub fn open_page(
    app: AppHandle,
    state: State<App>,
    page: String,
    fields: std::collections::BTreeMap<String, String>,
) -> CommandResult<()> {
    let url = match page.as_str() {
        "releases" => format!("{REPO}/releases/latest"),
        "bug" | "idea" => {
            let template = if page == "bug" { "bug.yml" } else { "idea.yml" };
            let mut params = vec![
                ("template".to_owned(), template.to_owned()),
                ("version".to_owned(), current(&app)),
                ("os".to_owned(), format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)),
                ("install".to_owned(), format!("{:?}", install_kind())),
            ];
            for (k, v) in fields {
                if !matches!(k.as_str(), "theme" | "motion" | "errors") {
                    continue;
                }
                // Error messages are ours, but run the checker anyway: drop
                // any line that could identify a person.
                let year = i32::from(Timestamp::now().to_zoned(state.tz.clone()).year());
                let lines: Vec<String> = v
                    .lines()
                    .filter(|l| sachiva_core::checker::check(l, year).is_empty())
                    .map(|l| l.chars().take(300).collect())
                    .take(3)
                    .collect();
                params.push((k, lines.join("\n")));
            }
            tauri::Url::parse_with_params(&format!("{REPO}/issues/new"), &params).map_err(err)?.to_string()
        }
        _ => return Err("Unknown page.".into()),
    };
    app.opener().open_url(url, None::<&str>).map_err(err)
}

#[cfg(desktop)]
fn updater(app: &AppHandle, kind: InstallKind, exact: Option<String>) -> CommandResult<tauri_plugin_updater::Updater> {
    use tauri_plugin_updater::UpdaterExt;
    let mut b = app.updater_builder().timeout(std::time::Duration::from_secs(30));
    if kind == InstallKind::Pacman {
        b = b.target("linux-x86_64-pacman");
    }
    if let Some(v) = exact {
        // That release's own manifest, so going back is checked like going
        // forward: .../releases/latest/download/latest.json becomes
        // .../releases/download/v<version>/latest.json.
        let latest = app
            .config()
            .plugins
            .0
            .get("updater")
            .and_then(|u| u.get("endpoints")?.get(0)?.as_str().map(str::to_owned))
            .ok_or("No update address is configured.")?;
        let url =
            latest.replace("/releases/latest/download/", &format!("/releases/download/v{v}/")).parse().map_err(err)?;
        b = b.endpoints(vec![url]).map_err(err)?.version_comparator(move |_, r| r.version.to_string() == v);
    }
    b.build().map_err(err)
}

#[cfg(mobile)]
#[derive(Deserialize)]
struct Manifest {
    version: String,
    notes: Option<String>,
    pub_date: Option<String>,
}

/// Is there a newer version? Checks even when this copy cannot install it,
/// so the portable version can say so.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> CommandResult<Option<UpdateInfo>> {
    let kind = install_kind();
    #[cfg(desktop)]
    {
        let found =
            updater(&app, kind, None)?.check().await.map_err(|e| format!("Could not check for updates ({e})."))?;
        Ok(found.map(|u| UpdateInfo {
            version: u.version,
            notes: u.body,
            date: u.date.map(|d| d.to_string()),
            can_install: kind.can_install(),
        }))
    }
    #[cfg(mobile)]
    {
        use tauri_plugin_sachiva_device::DeviceExt;
        let text = app.device().fetch_text(&format!("{REPO}/releases/latest/download/latest.json")).map_err(err)?;
        let m: Manifest =
            serde_json::from_str(&text).map_err(|e| format!("The update list could not be read ({e})."))?;
        Ok(newer(&m.version, &current(&app)).then(|| UpdateInfo {
            version: m.version,
            notes: m.notes,
            date: m.pub_date,
            can_install: kind.can_install(),
        }))
    }
}

/// Download, check and install `version` (forward, or back when `back`).
async fn install(app: &AppHandle, state: &App, version: &str, back: bool) -> CommandResult<()> {
    let kind = install_kind();
    if !kind.can_install() {
        return Err("This copy of Sachiva cannot update itself. Download the new version instead.".into());
    }
    let version = plain_version(version)?.to_owned();
    #[cfg(desktop)]
    {
        use tauri::Emitter;
        let exact = back.then(|| version.clone());
        let update = updater(app, kind, exact)?
            .check()
            .await
            .map_err(|e| format!("Could not reach the update server ({e})."))?
            .filter(|u| u.version == version)
            .ok_or(format!("Version {version} is not available."))?;
        let mut done = 0usize;
        let progress = app.clone();
        let bytes = update
            .download(
                move |n, total| {
                    done += n;
                    let _ = progress.emit("update-progress", (done, total));
                },
                || {},
            )
            .await
            .map_err(|e| format!("The download failed or did not pass the signature check ({e})."))?;
        before_install(app, state)?;
        if kind == InstallKind::Pacman {
            install_pacman(state, &version, &bytes)?;
        } else {
            // On Windows this starts the installer and closes Sachiva.
            update.install(&bytes).map_err(|e| format!("The update could not be installed ({e})."))?;
        }
        app.restart();
    }
    #[cfg(mobile)]
    {
        use tauri_plugin_sachiva_device::DeviceExt;
        if back {
            return Err("Android cannot install an older version over a newer one.".into());
        }
        before_install(app, state)?;
        let name = format!("Sachiva_{version}_aarch64.apk");
        let base = format!("{REPO}/releases/download/v{version}");
        app.device().install_apk(&format!("{base}/{name}"), &format!("{base}/SHA256SUMS.txt"), &name).map_err(err)?;
        Ok(())
    }
}

/// Install the signed package with pacman, asking for the password through
/// the desktop's own prompt (pkexec).
#[cfg(desktop)]
fn install_pacman(state: &App, version: &str, bytes: &[u8]) -> CommandResult<()> {
    let dir = state.dir.join("updates");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(err)?;
    let file = dir.join(format!("sachiva-{version}-x86_64.pkg.tar.zst"));
    std::fs::write(&file, bytes).map_err(err)?;
    let out = std::process::Command::new("pkexec")
        .args(["pacman", "-U", "--noconfirm"])
        .arg(&file)
        .output()
        .map_err(|e| format!("Could not ask for the password to install ({e}). Is polkit installed?"))?;
    let _ = std::fs::remove_dir_all(&dir);
    if !out.status.success() {
        return Err(match out.status.code() {
            Some(126) | Some(127) => "Installing was cancelled: the password was not given.".into(),
            _ => format!("pacman could not install the update: {}", String::from_utf8_lossy(&out.stderr).trim()),
        });
    }
    Ok(())
}

#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, App>, version: String) -> CommandResult<()> {
    install(&app, &state, &version, false).await
}

#[tauri::command]
pub fn rollback_plan(state: State<App>) -> CommandResult<Option<RollbackPlan>> {
    let Some(version) = previous(&state) else { return Ok(None) };
    let backup = pre_update_backup_of(&state, &version);
    let now = state.with(|s| s.schema_version())?;
    let needs_restore = match &backup {
        Some(b) => {
            let key = state.inner.lock().map_err(err)?.key.ok_or("Sachiva is locked.")?;
            let path = state.backup_dir().join(b);
            sachiva_core::store::inspect(&path, sachiva_core::store::Secret::Key(&key), &state.dir)
                .map(|s| s.schema < now)
                .unwrap_or(true)
        }
        // Without the backup we cannot tell; the older version refuses data
        // it cannot read, and offers a restore.
        None => false,
    };
    Ok(Some(RollbackPlan { version, backup, needs_restore }))
}

/// Go back to the version before the last update. When that version cannot
/// read today's data, `restore` must be true: the pre-update backup then
/// replaces the data (the current data is backed up first).
#[tauri::command]
pub async fn rollback(app: AppHandle, state: State<'_, App>, restore: bool) -> CommandResult<()> {
    let plan = rollback_plan(state.clone())?.ok_or("There is no earlier version to go back to.")?;
    if plan.needs_restore {
        if !restore {
            return Err("Going back needs the backup from before the update. Confirm to use it.".into());
        }
        let name = plan.backup.as_deref().ok_or("The backup from before the update is missing.")?;
        state.stage_restore(name)?;
    }
    let r = install(&app, &state, &plan.version, true).await;
    if r.is_err() && plan.needs_restore {
        state.cancel_staged_restore();
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers() {
        assert!(newer("0.1.10", "0.1.9"));
        assert!(newer("v1.0.0", "0.9.9"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("garbage", "0.0.1"));
    }

    #[test]
    fn only_plain_versions_reach_a_url() {
        assert!(plain_version("0.1.2").is_ok());
        for bad in ["", "0.1.2/../../x", "0.1.2?x", "v0.1.2"] {
            assert!(plain_version(bad).is_err(), "{bad}");
        }
    }
}
