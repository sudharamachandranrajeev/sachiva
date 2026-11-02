use crate::{App, CommandResult, changed};
use sachiva_core::{jiff::Timestamp, store::focus::FocusSession};
use tauri::{AppHandle, State};

#[tauri::command]
pub fn focus_sessions(state: State<App>) -> CommandResult<Vec<FocusSession>> {
    state.with(|s| s.focus_sessions())
}

#[tauri::command]
pub fn start_focus(app: AppHandle, state: State<App>, task_id: String) -> CommandResult<FocusSession> {
    let session = state.with(|s| s.start_focus(&task_id, Timestamp::now()))?;
    changed(&app);
    Ok(session)
}

#[tauri::command]
pub fn stop_focus(app: AppHandle, state: State<App>, id: String) -> CommandResult<FocusSession> {
    let session = state.with(|s| s.stop_focus(&id, Timestamp::now()))?;
    changed(&app);
    Ok(session)
}
