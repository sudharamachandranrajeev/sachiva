use crate::{App, CommandResult, changed};
use sachiva_core::{
    jiff::Timestamp,
    model::{Kind, Task},
    store::calendar::{CalendarItem, CalendarResult},
};
use serde::Deserialize;
use tauri::{AppHandle, State};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDraft {
    uid: String,
    title: String,
    notes: String,
    when: Timestamp,
    kind: CalendarKind,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CalendarKind {
    Appointment,
    Deadline,
}

#[tauri::command]
pub fn import_calendar(app: AppHandle, state: State<App>, drafts: Vec<CalendarDraft>) -> CommandResult<CalendarResult> {
    let now = Timestamp::now();
    let items = drafts
        .into_iter()
        .map(|draft| {
            let mut task = Task::new(draft.title.trim(), now);
            task.notes = draft.notes;
            match draft.kind {
                CalendarKind::Appointment => {
                    task.kind = Kind::Appointment;
                    task.appointment = Some(draft.when);
                }
                CalendarKind::Deadline => {
                    task.kind = Kind::Deadline;
                    task.due = Some(draft.when);
                }
            }
            CalendarItem { uid: draft.uid, task }
        })
        .collect();
    let result = state.with(|s| s.import_calendar(items, now))?;
    if result.imported > 0 {
        changed(&app);
    }
    Ok(result)
}
