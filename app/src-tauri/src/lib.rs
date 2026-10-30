//! Sachiva's app shell: opens the encrypted store with the device key, answers
//! the UI's commands, and fires reminders. On the desktop a tray icon keeps
//! it running so reminders arrive with the window closed; on Android the
//! next few days of reminders are handed to the system as exact alarms.

mod data;
mod targets;
mod update;

use std::path::PathBuf;
use std::sync::Mutex;

use sachiva_core::checker::{self, Finding};
use sachiva_core::dashboard::{self, Dashboard};
use sachiva_core::jiff::tz::TimeZone;
use sachiva_core::jiff::{SignedDuration, Timestamp};
use sachiva_core::model::{Status, Task};
use sachiva_core::reminders::{self, Reminder};
use sachiva_core::store::{Store, Trashed};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_sachiva_device::{AlarmAccess, DeviceExt, KeyStorage, from_hex, to_hex};

const DB_FILE: &str = "sachiva.db";
const KEEP_BACKUPS: usize = 14;
const TRASH_DAYS: i64 = 30;

type CommandResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

struct Inner {
    store: Option<Store>,
    key: Option<[u8; 32]>,
    key_storage: Option<KeyStorage>,
    /// Why the store is not open, in plain words.
    problem: Option<String>,
    /// Something to tell the user once, such as a failed restore at start.
    notice: Option<String>,
}

struct App {
    inner: Mutex<Inner>,
    dir: PathBuf,
    tz: TimeZone,
}

impl App {
    fn with<T>(&self, f: impl FnOnce(&mut Store) -> sachiva_core::store::Result<T>) -> CommandResult<T> {
        let mut inner = self.inner.lock().map_err(err)?;
        let store = inner.store.as_mut().ok_or("Sachiva is locked: enter your recovery key first.")?;
        f(store).map_err(err)
    }

    fn settings(&self) -> reminders::Settings {
        self.with(|s| s.get("reminders.settings")).ok().flatten().unwrap_or_default()
    }

    /// Open the store with the key from this device.
    fn unlock(&self, app: &AppHandle) {
        let db = self.dir.join(DB_FILE);
        let first = !db.exists();
        let staged = |k: &[u8; 32]| self.apply_staged_restore(k);
        let mut inner = self.inner.lock().unwrap();
        let key = app.device().db_key(&self.dir, first);
        if let Ok(k) = &key {
            inner.notice = staged(&k.key);
        }
        match key {
            Ok(k) => match Store::open(&db, &k.key) {
                Ok(store) => {
                    *inner = Inner {
                        store: Some(store),
                        key: Some(k.key),
                        key_storage: Some(k.storage),
                        problem: None,
                        notice: inner.notice.take(),
                    }
                }
                Err(e) => inner.problem = Some(format!("Your data could not be opened: {e}")),
            },
            Err(e) => inner.problem = Some(format!("{e}.")),
        }
    }

    /// Daily backup and trash clean-up, once per local day.
    fn maintain(&self) {
        let now = Timestamp::now();
        let today = now.to_zoned(self.tz.clone()).date().to_string();
        let inner = self.inner.lock().unwrap();
        let (Some(store), Some(key)) = (inner.store.as_ref(), inner.key.as_ref()) else { return };
        if store.get::<String>("maintenance.day").ok().flatten().as_deref() == Some(today.as_str()) {
            return;
        }
        let _ = store.daily_backup(&self.backup_dir(), key, now, KEEP_BACKUPS);
        drop(inner);
        let _ = self.with(|s| {
            s.purge(now, TRASH_DAYS)?;
            s.set("maintenance.day", &today)
        });
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusView {
    unlocked: bool,
    problem: Option<String>,
    /// The user has not yet confirmed they saved the recovery key.
    recovery_pending: bool,
    key_storage: Option<KeyStorage>,
    platform: &'static str,
    notice: Option<String>,
    /// The first-run steps after the recovery key are done or skipped.
    setup_done: bool,
}

#[tauri::command]
fn status(state: State<App>) -> StatusView {
    let mut inner = state.inner.lock().unwrap();
    let setup_done = inner.store.as_ref().and_then(|s| s.get::<bool>("setup.done").ok().flatten()).unwrap_or(false);
    let saved = inner.store.as_ref().and_then(|s| s.get::<bool>("recovery.saved").ok().flatten()).unwrap_or(false);
    StatusView {
        unlocked: inner.store.is_some(),
        problem: inner.problem.clone(),
        recovery_pending: inner.store.is_some() && !saved,
        key_storage: inner.key_storage,
        platform: std::env::consts::OS,
        notice: inner.notice.take(),
        setup_done,
    }
}

#[tauri::command]
fn setup_done(state: State<App>) -> CommandResult<()> {
    state.with(|s| s.set("setup.done", &true))
}

/// Does Sachiva start with the computer? Always false on Android.
#[tauri::command]
fn autostart(app: AppHandle) -> bool {
    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt;
        app.autolaunch().is_enabled().unwrap_or(false)
    }
    #[cfg(mobile)]
    {
        let _ = app;
        false
    }
}

#[tauri::command]
fn set_autostart(app: AppHandle, on: bool) -> CommandResult<()> {
    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt;
        let a = app.autolaunch();
        if on { a.enable() } else { a.disable() }.map_err(err)
    }
    #[cfg(mobile)]
    {
        let _ = (app, on);
        Ok(())
    }
}

/// The key in groups of four, for writing down.
#[tauri::command]
fn recovery_key(state: State<App>) -> CommandResult<String> {
    let inner = state.inner.lock().map_err(err)?;
    let key = inner.key.as_ref().ok_or("Sachiva is locked.")?;
    let hex = to_hex(key);
    Ok(hex.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join("-"))
}

#[tauri::command]
fn recovery_saved(state: State<App>) -> CommandResult<()> {
    state.with(|s| s.set("recovery.saved", &true))
}

/// Unlock with the recovery key, then keep it on this device again.
#[tauri::command]
fn unlock_with(app: AppHandle, state: State<App>, recovery: String) -> CommandResult<()> {
    let key = from_hex(&recovery).ok_or("A recovery key is 64 letters and digits (0-9, a-f).")?;
    let store =
        Store::open(&state.dir.join(DB_FILE), &key).map_err(|_| "That recovery key does not open this data.")?;
    let storage = app.device().set_db_key(&state.dir, &key).map_err(err)?;
    *state.inner.lock().map_err(err)? =
        Inner { store: Some(store), key: Some(key), key_storage: Some(storage), problem: None, notice: None };
    changed(&app);
    Ok(())
}

#[tauri::command]
fn get_dashboard(state: State<App>) -> CommandResult<Dashboard> {
    let tasks = state.with(|s| s.tasks())?;
    Ok(dashboard::build(&tasks, Timestamp::now(), &state.tz, state.settings().urgent_score))
}

/// Finished jobs, newest first.
#[tauri::command]
fn done_tasks(state: State<App>) -> CommandResult<Vec<Task>> {
    let mut done: Vec<Task> = state.with(|s| s.tasks())?.into_iter().filter(|t| t.is_done()).collect();
    done.sort_by_key(|t| std::cmp::Reverse(t.updated_at));
    done.truncate(200);
    Ok(done)
}

#[tauri::command]
fn new_task(title: String) -> Task {
    Task::new(title, Timestamp::now())
}

#[tauri::command]
fn save_task(app: AppHandle, state: State<App>, mut task: Task) -> CommandResult<Task> {
    task.title = task.title.trim().to_string();
    task.validate()?;
    let now = Timestamp::now();
    task.updated_at = now;
    state.with(|s| s.save(&task, now))?;
    changed(&app);
    Ok(task)
}

#[tauri::command]
fn complete(app: AppHandle, state: State<App>, id: String) -> CommandResult<()> {
    state.with(|s| s.complete(&id, Timestamp::now(), &state.tz))?;
    changed(&app);
    Ok(())
}

/// Put a finished job back on the list.
#[tauri::command]
fn reopen(app: AppHandle, state: State<App>, id: String) -> CommandResult<()> {
    let now = Timestamp::now();
    state.with(|s| {
        let mut t = s.task(&id)?;
        t.status = Status::Open;
        t.updated_at = now;
        s.save(&t, now)
    })?;
    changed(&app);
    Ok(())
}

#[tauri::command]
fn delete_task(app: AppHandle, state: State<App>, id: String) -> CommandResult<()> {
    state.with(|s| s.delete(&id, Timestamp::now()))?;
    changed(&app);
    Ok(())
}

#[tauri::command]
fn restore_task(app: AppHandle, state: State<App>, id: String) -> CommandResult<()> {
    state.with(|s| s.restore(&id, Timestamp::now()))?;
    changed(&app);
    Ok(())
}

#[tauri::command]
fn trash(state: State<App>) -> CommandResult<Vec<Trashed>> {
    state.with(|s| s.trash())
}

/// Undo the last change. Returns what was undone, or nothing.
#[tauri::command]
fn undo(app: AppHandle, state: State<App>) -> CommandResult<Option<String>> {
    let undone = state.with(|s| s.undo())?;
    changed(&app);
    Ok(undone.map(|u| u.label))
}

/// Quiet a job's reminders for `minutes`. "I'm on it" is 60.
#[tauri::command]
fn snooze(app: AppHandle, state: State<App>, id: String, minutes: i64) -> CommandResult<()> {
    if !(1..=60 * 24 * 14).contains(&minutes) {
        return Err("Snooze for between a minute and two weeks.".into());
    }
    let now = Timestamp::now();
    state.with(|s| {
        let mut t = s.task(&id)?;
        t.snoozed_until = Some(now + SignedDuration::from_mins(minutes));
        // Not an edit: keep updated_at so the quiet-after-edit rule is not reset.
        s.save(&t, now)
    })?;
    changed(&app);
    Ok(())
}

#[tauri::command]
fn check_text(state: State<App>, text: String) -> Vec<Finding> {
    let year = Timestamp::now().to_zoned(state.tz.clone()).year();
    checker::check(&text, i32::from(year))
}

#[tauri::command]
fn get_settings(state: State<App>) -> reminders::Settings {
    state.settings()
}

#[tauri::command]
fn set_settings(app: AppHandle, state: State<App>, settings: reminders::Settings) -> CommandResult<()> {
    if settings.work_start >= settings.work_end {
        return Err("Working hours must start before they end.".into());
    }
    if !(1..=24 * 60).contains(&settings.repeat_every_min) || !(1..=24 * 60).contains(&settings.urgent_every_min) {
        return Err("Repeats must be between 1 minute and a day apart.".into());
    }
    if !(0..=24 * 60).contains(&settings.quiet_after_edit_min) {
        return Err("The quiet time after an edit can be at most a day.".into());
    }
    state.with(|s| s.set("reminders.settings", &settings))?;
    changed(&app);
    Ok(())
}

#[tauri::command]
fn alarm_access(app: AppHandle) -> CommandResult<AlarmAccess> {
    app.device().alarm_access().map_err(err)
}

#[tauri::command]
fn open_alarm_settings(app: AppHandle) -> CommandResult<()> {
    app.device().open_alarm_settings().map_err(err)
}

#[tauri::command]
fn open_battery_settings(app: AppHandle) -> CommandResult<()> {
    app.device().open_battery_settings().map_err(err)
}

/// Data changed: tell the UI and plan reminders again.
fn changed(app: &AppHandle) {
    let _ = app.emit("changed", ());
    #[cfg(mobile)]
    schedule_alarms(app);
}

/// Reminders due in `(from, to]`, at most one per task (the latest).
fn due_reminders(state: &App, from: Timestamp, to: Timestamp) -> Vec<Reminder> {
    let settings = state.settings();
    let Ok(tasks) = state.with(|s| s.tasks()) else { return Vec::new() };
    let mut due: Vec<Reminder> = sachiva_core::priority::rank(&tasks, to)
        .into_iter()
        .filter_map(|(t, p)| reminders::between(t, &p, from, to, &settings, &state.tz).pop())
        .collect();
    // Targets are not ranked; behind-pace nudges come separately, the latest per target.
    let mut pace = targets::pace_reminders(state, from, to, &settings);
    pace.sort_by_key(|r| r.at);
    for r in pace.into_iter().rev() {
        if !due.iter().any(|d| d.task_id == r.task_id) {
            due.push(r);
        }
    }
    due
}

#[cfg(desktop)]
fn reminder_loop(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            let state = app.state::<App>();
            state.maintain();
            let now = Timestamp::now();
            let last: Option<Timestamp> = state.with(|s| s.get("reminders.lastTick")).ok().flatten();
            // After a long time closed, catch up on the last day only, one per job.
            let from = last.unwrap_or(now - SignedDuration::from_secs(60)).max(now - SignedDuration::from_hours(24));
            for r in due_reminders(&state, from, now) {
                let title = if r.urgent { format!("Urgent · {}", r.title) } else { r.title.clone() };
                let _ = app.notification().builder().title(title).body(r.body).show();
            }
            let _ = state.with(|s| s.set("reminders.lastTick", &now));
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
    });
}

/// Android: hand the next days' reminders to the system as exact alarms.
#[cfg(mobile)]
fn schedule_alarms(app: &AppHandle) {
    use tauri_plugin_notification::Schedule;
    const DAYS: i64 = 3;
    const MAX: usize = 400;
    let state = app.state::<App>();
    let now = Timestamp::now();
    let settings = state.settings();
    let Ok(tasks) = state.with(|s| s.tasks()) else { return };
    let mut all: Vec<Reminder> = sachiva_core::priority::rank(&tasks, now)
        .into_iter()
        .flat_map(|(t, p)| {
            reminders::between(t, &p, now, now + SignedDuration::from_hours(24 * DAYS), &settings, &state.tz)
        })
        .collect();
    all.extend(targets::pace_reminders(&state, now, now + SignedDuration::from_hours(24 * DAYS), &settings));
    all.sort_by_key(|r| r.at);
    all.truncate(MAX);

    let old: Vec<i32> = state.with(|s| s.get("reminders.scheduled")).ok().flatten().unwrap_or_default();
    let _ = app.notification().cancel(old);
    let at = |t: Timestamp| time::OffsetDateTime::from_unix_timestamp(t.as_second()).ok();
    let mut ids = Vec::new();
    for (i, r) in all.iter().enumerate() {
        let Some(date) = at(r.at) else { continue };
        let id = 1000 + i as i32;
        let mut b = app
            .notification()
            .builder()
            .id(id)
            .title(&r.title)
            .body(&r.body)
            .channel_id(if r.urgent { "urgent" } else { "reminders" })
            .schedule(Schedule::At { date, repeating: false, allow_while_idle: true });
        if r.urgent {
            b = b.ongoing();
        } else {
            b = b.auto_cancel();
        }
        if b.show().is_ok() {
            ids.push(id);
        }
    }
    // Alarms are planned only a few days ahead: a nudge to open the app keeps them going.
    if tasks.iter().any(|t| !t.is_done())
        && let Some(date) = at(now + SignedDuration::from_hours(24 * DAYS - 2))
    {
        let id = 999;
        let _ = app
            .notification()
            .builder()
            .id(id)
            .title("Open Sachiva to keep reminders coming")
            .body("Reminders are planned a few days ahead. Opening the app plans the next days.")
            .channel_id("reminders")
            .schedule(Schedule::At { date, repeating: false, allow_while_idle: true })
            .show();
        ids.push(id);
    }
    let _ = state.with(|s| s.set("reminders.scheduled", &ids));
}

#[cfg(mobile)]
fn create_channels(app: &AppHandle) {
    use tauri_plugin_notification::{Channel, Importance};
    let n = app.notification();
    let _ = n.create_channel(
        Channel::builder("reminders", "Reminders")
            .description("Coming up, due and overdue jobs")
            .importance(Importance::High)
            .build(),
    );
    let _ = n.create_channel(
        Channel::builder("urgent", "Urgent jobs")
            .description("Stays until you act on it, and repeats")
            .importance(Importance::High)
            .vibration(true)
            .lights(true)
            .build(),
    );
}

#[cfg(desktop)]
fn tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    let open = MenuItem::with_id(app, "open", "Open Sachiva", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit (stops reminders)", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main").tooltip("Sachiva").menu(&menu).show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.on_menu_event(|app, e| match e.id.as_ref() {
        "open" => show(app),
        "quit" => app.exit(0),
        _ => {}
    })
    .on_tray_icon_event(|tray, e| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
            show(tray.app_handle());
        }
    })
    .build(app)?;
    Ok(())
}

#[cfg(desktop)]
fn show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // For scripts and release checks: answers without opening a window.
    if std::env::args().any(|a| a == "--version") {
        println!("Sachiva {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let mut builder = tauri::Builder::default();
    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show(app)))
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec!["--hidden"]),
            ))
            .plugin(tauri_plugin_updater::Builder::new().build());
    }
    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_sachiva_device::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            // The database is encrypted; keep the folder private too.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
            }
            let tz = TimeZone::try_system().unwrap_or(TimeZone::UTC);
            app.manage(App {
                inner: Mutex::new(Inner { store: None, key: None, key_storage: None, problem: None, notice: None }),
                dir,
                tz,
            });
            let handle = app.handle().clone();
            app.state::<App>().unlock(&handle);
            #[cfg(desktop)]
            {
                use tauri_plugin_autostart::ManagerExt;
                tray(app)?;
                // Start with the computer by default, so reminders keep coming.
                let state = app.state::<App>();
                if state.with(|s| s.get::<bool>("autostart.asked")).ok().flatten().is_none() {
                    let _ = app.autolaunch().enable();
                    let _ = state.with(|s| s.set("autostart.asked", &true));
                }
                if std::env::args().any(|a| a == "--hidden")
                    && let Some(w) = app.get_webview_window("main")
                {
                    let _ = w.hide();
                }
                reminder_loop(handle.clone());
            }
            #[cfg(mobile)]
            {
                let _ = handle.notification().request_permission();
                create_channels(&handle);
                app.state::<App>().maintain();
                schedule_alarms(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Sachiva in the tray, so reminders still arrive.
            #[cfg(desktop)]
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
            #[cfg(mobile)]
            let _ = (window, event);
        })
        .invoke_handler(tauri::generate_handler![
            status,
            recovery_key,
            recovery_saved,
            unlock_with,
            get_dashboard,
            done_tasks,
            new_task,
            save_task,
            complete,
            reopen,
            delete_task,
            restore_task,
            trash,
            undo,
            snooze,
            check_text,
            get_settings,
            set_settings,
            setup_done,
            autostart,
            set_autostart,
            data::backups,
            data::backup_now,
            data::export_backup,
            data::pick_backup,
            data::inspect_backup,
            data::restore_backup,
            update::update_status,
            update::set_update_mode,
            update::check_update,
            update::install_update,
            update::rollback_plan,
            update::rollback,
            update::open_page,
            targets::counters,
            targets::add_count,
            targets::recent_counts,
            targets::waitlists,
            targets::new_waitlist,
            targets::save_waitlist,
            targets::get_figures,
            targets::save_figures,
            targets::preview_report,
            targets::export_report,
            alarm_access,
            open_alarm_settings,
            open_battery_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Sachiva");
}
