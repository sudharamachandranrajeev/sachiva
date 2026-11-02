use crate::{App, CommandResult, changed};
use sachiva_core::{
    jiff::Timestamp,
    model::Task,
    waitlist::WaitList,
    work::{self, Template, WorkflowKind},
};
use serde::Serialize;
use tauri::{AppHandle, State};

#[derive(Serialize)]
pub struct WorkData {
    tasks: Vec<Task>,
    templates: Vec<Template>,
    lists: Vec<WaitList>,
}
#[tauri::command]
pub fn work_data(state: State<App>) -> CommandResult<WorkData> {
    state.with(|s| Ok(WorkData { tasks: s.tasks()?, templates: s.templates()?, lists: s.waitlists()? }))
}
#[tauri::command]
pub fn new_workflow_task(state: State<App>, kind: WorkflowKind, list_id: Option<String>) -> CommandResult<Task> {
    let mut t = work::new_task(kind, list_id.clone(), Timestamp::now());
    if kind == WorkflowKind::Session {
        let id = list_id.ok_or("Add a clinic list first.")?;
        t.site = state.with(|s| s.waitlist(&id))?.site;
    }
    Ok(t)
}
#[tauri::command]
pub fn create_template(app: AppHandle, state: State<App>, task_id: String, name: String) -> CommandResult<Template> {
    let t = state.with(|s| s.create_template(&task_id, &name, Timestamp::now()))?;
    changed(&app);
    Ok(t)
}
#[tauri::command]
pub fn archive_template(
    app: AppHandle,
    state: State<App>,
    id: String,
    revision: u32,
    archived: bool,
) -> CommandResult<()> {
    state.with(|s| s.archive_template(&id, revision, archived, Timestamp::now()))?;
    changed(&app);
    Ok(())
}
#[tauri::command]
pub fn task_from_template(state: State<App>, id: String) -> CommandResult<Task> {
    state.with(|s| s.task_from_template(&id, Timestamp::now()))
}
#[tauri::command]
pub fn confirm_delivery(app: AppHandle, state: State<App>, id: String) -> CommandResult<()> {
    state.with(|s| s.confirm_delivery(&id, Timestamp::now()))?;
    changed(&app);
    Ok(())
}
#[tauri::command]
pub fn cancel_session(app: AppHandle, state: State<App>, id: String) -> CommandResult<()> {
    state.with(|s| s.cancel_session(&id, Timestamp::now()))?;
    changed(&app);
    Ok(())
}
