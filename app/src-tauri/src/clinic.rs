//! Commands for the reference-only clinic desk.

use crate::{App, CommandResult, changed};
use sachiva_core::{
    clinic::{self, Candidate, ContactInput, Entry, Slot, Summary},
    jiff::{Timestamp, civil::Date},
    reminders::{Reminder, Settings},
    store,
    waitlist::WaitList,
};
use serde::Serialize;
use tauri::{AppHandle, State};

#[derive(Serialize)]
pub struct ClinicData {
    lists: Vec<WaitList>,
    entries: Vec<Entry>,
    slots: Vec<Slot>,
    summary: Summary,
}

#[tauri::command]
pub fn clinic_data(state: State<App>) -> CommandResult<ClinicData> {
    state.with(|s| {
        let slots = s.clinic_slots()?;
        Ok(ClinicData { lists: s.waitlists()?, entries: s.clinic_entries()?, summary: clinic::summary(&slots), slots })
    })
}

#[tauri::command]
pub fn clinic_summary(state: State<App>) -> CommandResult<Summary> {
    state.with(|s| Ok(clinic::summary(&s.clinic_slots()?)))
}

#[tauri::command]
pub fn clinic_candidates(state: State<App>, slot_id: String) -> CommandResult<Vec<Candidate>> {
    state.with(|s| {
        let slot =
            s.clinic_slots()?.into_iter().find(|s| s.id == slot_id).ok_or_else(|| store::Error::NotFound(slot_id))?;
        Ok(clinic::candidates(&s.clinic_entries()?, &slot, Timestamp::now(), &state.tz))
    })
}

#[tauri::command]
pub fn new_clinic_entry(list_id: String, reference: String, target: Date) -> Entry {
    Entry::new(list_id, reference, target, Timestamp::now())
}

#[tauri::command]
pub fn new_clinic_slot(list_id: String, start: Timestamp) -> Slot {
    Slot::new(list_id, start, Timestamp::now())
}

#[tauri::command]
pub fn save_clinic_entry(app: AppHandle, state: State<App>, entry: Entry) -> CommandResult<Entry> {
    let saved = state.with(|s| s.save_clinic_entry(entry, Timestamp::now()))?;
    changed(&app);
    Ok(saved)
}

#[tauri::command]
pub fn save_clinic_slot(app: AppHandle, state: State<App>, slot: Slot) -> CommandResult<Slot> {
    let saved = state.with(|s| s.save_clinic_slot(slot, Timestamp::now()))?;
    changed(&app);
    Ok(saved)
}

#[tauri::command]
pub fn contact_clinic_entry(
    app: AppHandle,
    state: State<App>,
    slot_id: String,
    revision: u32,
    contact: ContactInput,
) -> CommandResult<()> {
    state.with(|s| s.contact_clinic_entry(&slot_id, revision, contact, Timestamp::now(), &state.tz))?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn close_clinic_slot(
    app: AppHandle,
    state: State<App>,
    slot_id: String,
    revision: u32,
    entry_id: Option<String>,
) -> CommandResult<()> {
    state.with(|s| s.close_clinic_slot(&slot_id, revision, entry_id.as_deref(), Timestamp::now(), &state.tz))?;
    changed(&app);
    Ok(())
}

pub fn planned(state: &App, from: Timestamp, to: Timestamp, settings: &Settings) -> Vec<Reminder> {
    state
        .with(|s| {
            let lists = s.waitlists()?;
            Ok(s.clinic_slots()?
                .iter()
                .flat_map(|slot| {
                    let name = lists.iter().find(|l| l.id == slot.list_id).map_or("Clinic", |l| l.name.as_str());
                    clinic::reminders(slot, name, from, to, settings, &state.tz)
                })
                .collect())
        })
        .unwrap_or_default()
}
