//! Targets, tallies, waiting lists and reports: the commands behind the
//! dashboard counters, the waiting-list screen and the report exports.

use sachiva_core::counts::{self, Count, Progress};
use sachiva_core::jiff::civil::Date;
use sachiva_core::jiff::{SignedDuration, Timestamp};
use sachiva_core::model::{Kind, Span, Task};
use sachiva_core::reminders::{self, Reminder};
use sachiva_core::report::{self, Report, ReportKind, Request};
use sachiva_core::waitlist::{Figures, WaitList};
use sachiva_export::Format;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::{FsExt, OpenOptions};

use crate::{App, CommandResult, changed, err};

#[derive(Serialize)]
pub struct Counter {
    task: Task,
    progress: Progress,
}

/// Every open target and tally with its progress: those behind first,
/// then by title.
#[tauri::command]
pub fn counters(state: State<App>) -> CommandResult<Vec<Counter>> {
    let now = Timestamp::now();
    let mut out = state.with(|s| {
        let mut out = Vec::new();
        for t in s.tasks()?.into_iter().filter(|t| !t.is_done()) {
            if matches!(t.kind, Kind::Target { .. } | Kind::Tally { .. })
                && let Some(progress) = counts::progress(&t, &s.counts_of(&t.id)?, now, &state.tz)
            {
                out.push(Counter { task: t, progress });
            }
        }
        Ok(out)
    })?;
    out.sort_by(|a, b| b.progress.behind.cmp(&a.progress.behind).then(a.task.title.cmp(&b.task.title)));
    Ok(out)
}

/// Add to a count, or take away with a negative `delta` (a correction is a
/// new entry, so nothing is ever lost). Returns the new progress.
#[tauri::command]
pub fn add_count(app: AppHandle, state: State<App>, id: String, delta: i64, note: String) -> CommandResult<Progress> {
    let now = Timestamp::now();
    let p = state.with(|s| {
        s.add_count(&id, delta, &note, now)?;
        let t = s.task(&id)?;
        Ok(counts::progress(&t, &s.counts_of(&id)?, now, &state.tz))
    })?;
    changed(&app);
    p.ok_or_else(|| "This job does not keep a count.".into())
}

/// The latest entries of one count, newest first, for the undo list.
#[tauri::command]
pub fn recent_counts(state: State<App>, id: String) -> CommandResult<Vec<Count>> {
    let mut v = state.with(|s| s.counts_of(&id))?;
    v.reverse();
    v.truncate(20);
    Ok(v)
}

#[tauri::command]
pub fn waitlists(state: State<App>) -> CommandResult<Vec<WaitList>> {
    state.with(|s| s.waitlists())
}

#[tauri::command]
pub fn new_waitlist(name: String) -> WaitList {
    WaitList::new(name)
}

#[tauri::command]
pub fn save_waitlist(app: AppHandle, state: State<App>, mut list: WaitList) -> CommandResult<WaitList> {
    for s in [&mut list.name, &mut list.site, &mut list.clinic, &mut list.provider] {
        *s = s.trim().to_string();
    }
    state.with(|s| s.save_waitlist(&list, Timestamp::now()))?;
    changed(&app);
    Ok(list)
}

/// The figures saved for one list and period, or blank ones to fill in.
#[tauri::command]
pub fn get_figures(state: State<App>, list_id: String, span: Span, start: Date) -> CommandResult<Figures> {
    let start = counts::period_start(span, start);
    state.with(|s| {
        let list = s.waitlist(&list_id)?;
        let saved = s.figures(span, start, start)?.into_iter().find(|f| f.list_id == list_id);
        Ok(saved.unwrap_or_else(|| Figures::empty(&list.id, span, start, list.bands.len() + 1)))
    })
}

#[tauri::command]
pub fn save_figures(app: AppHandle, state: State<App>, mut figures: Figures) -> CommandResult<()> {
    figures.note = figures.note.trim().to_string();
    state.with(|s| s.save_figures(&figures, Timestamp::now()))?;
    changed(&app);
    Ok(())
}

fn build(state: &App, req: &Request) -> CommandResult<Report> {
    req.validate()?;
    let now = Timestamp::now();
    state.with(|s| {
        Ok(match req.kind {
            ReportKind::Performance => {
                // Trashed jobs too, so their counts still have a name.
                let mut tasks = s.tasks()?;
                tasks.extend(s.trash()?.into_iter().map(|t| t.task));
                let since = counts::instant(counts::period_start(req.span, req.from), &state.tz);
                report::performance(req, &tasks, &s.counts_since(since)?, &state.tz, now)
            }
            ReportKind::WaitingList => {
                let from = counts::period_start(req.span, req.from);
                report::waiting(req, &s.waitlists()?, &s.figures(req.span, from, req.to)?, &state.tz, now)
            }
        })
    })
}

#[derive(Serialize)]
pub struct Preview {
    report: Report,
    /// The summary table as shown in the exports.
    summary: Vec<[String; 6]>,
    /// SVG text for each chart, per section; the same drawing as the exports.
    charts: Vec<Vec<String>>,
}

/// The report as it will be exported, for the on-screen preview.
#[tauri::command]
pub fn preview_report(state: State<App>, req: Request) -> CommandResult<Preview> {
    let report = build(&state, &req)?;
    let summary = sachiva_export::summary_rows(&report);
    let charts = report.sections.iter().map(|s| s.charts.iter().map(sachiva_export::chart::svg).collect()).collect();
    Ok(Preview { report, summary, charts })
}

/// Save the report as a file where the user picks. False when cancelled.
#[tauri::command]
pub async fn export_report(app: AppHandle, state: State<'_, App>, req: Request, format: Format) -> CommandResult<bool> {
    let report = build(&state, &req)?;
    let kind = match req.kind {
        ReportKind::Performance => "performance",
        ReportKind::WaitingList => "waiting-list",
    };
    let ext = format.extension();
    let name = format!("sachiva-{kind}-report-{}.{ext}", Timestamp::now().strftime("%Y-%m-%d"));
    let label = match format {
        Format::Xlsx => "Excel workbook",
        Format::Docx => "Word document",
        Format::Pptx => "PowerPoint presentation",
        Format::Pdf => "PDF document",
    };
    let dialog = app.dialog().file().set_file_name(&name).add_filter(label, &[ext]);
    let Some(target) = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file()).await.map_err(err)?
    else {
        return Ok(false);
    };
    let bytes = tauri::async_runtime::spawn_blocking(move || sachiva_export::export(&report, format))
        .await
        .map_err(err)?
        .map_err(err)?;
    let written = (|| -> std::io::Result<()> {
        use std::io::Write;
        let mut out = app.fs().open(target, OpenOptions::new().write(true).create(true).truncate(true).clone())?;
        out.write_all(&bytes)?;
        out.sync_all()
    })();
    written.map_err(|e| format!("The report could not be saved there ({e})."))?;
    Ok(true)
}

/// "Behind on" nudges for repeating targets in `(from, to]`.
pub fn pace_reminders(state: &App, from: Timestamp, to: Timestamp, s: &reminders::Settings) -> Vec<Reminder> {
    // A month back covers the longest period a target counts over.
    let since = from - SignedDuration::from_hours(24 * 32);
    let Ok((tasks, all)) = state.with(|st| Ok((st.tasks()?, st.counts_since(since)?))) else { return Vec::new() };
    tasks
        .iter()
        .filter(|t| !t.is_done())
        .flat_map(|t| {
            let mine: Vec<Count> = all.iter().filter(|c| c.task_id == t.id).cloned().collect();
            reminders::pace(t, &mine, from, to, s, &state.tz)
        })
        .collect()
}
