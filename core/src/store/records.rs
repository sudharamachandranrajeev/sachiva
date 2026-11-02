//! Closed set of JSON records sharing transactional undo with tasks.

use super::*;

pub(super) fn table(entity: &str) -> Result<&'static str> {
    match entity {
        "clinic_entry" => Ok("clinic_entries"),
        "clinic_slot" => Ok("clinic_slots"),
        "template" => Ok("task_templates"),
        _ => Err(Error::Invalid("Unknown history record type.".into())),
    }
}

pub(super) fn load<T: DeserializeOwned>(conn: &Connection, entity: &str, id: &str) -> Result<T> {
    let j: Option<String> =
        conn.query_row(&format!("SELECT json FROM {} WHERE id = ?", table(entity)?), [id], |r| r.get(0)).optional()?;
    Ok(serde_json::from_str(&j.ok_or_else(|| Error::NotFound(id.into()))?)?)
}

pub(super) fn all<T: DeserializeOwned>(conn: &Connection, entity: &str) -> Result<Vec<T>> {
    let mut st = conn.prepare(&format!("SELECT json FROM {} ORDER BY id", table(entity)?))?;
    st.query_map([], |r| r.get::<_, String>(0))?.map(|j| Ok(serde_json::from_str(&j?)?)).collect()
}

pub(super) fn put<T: Serialize>(
    tx: &rusqlite::Transaction,
    group: i64,
    entity: &str,
    id: &str,
    value: &T,
    label: &str,
    now: Timestamp,
) -> Result<()> {
    let name = table(entity)?;
    let before: Option<(String, i64)> = tx
        .query_row(&format!("SELECT json, updated_at FROM {name} WHERE id = ?"), [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let (json, updated) = before.map_or((None, None), |(j, u)| (Some(j), Some(u)));
    tx.execute(
        "INSERT INTO history (group_id,task_id,before_json,label,at,entity,before_updated_at) VALUES (?,?,?,?,?,?,?)",
        params![group, id, json, label, now.as_second(), entity, updated],
    )?;
    tx.execute(&format!("INSERT INTO {name} (id,json,updated_at) VALUES (?1,?2,?3) ON CONFLICT(id) DO UPDATE SET json=?2,updated_at=?3"), params![id,serde_json::to_string(value)?,now.as_second()])?;
    Ok(())
}

pub(super) fn undo_row(
    tx: &rusqlite::Transaction,
    entity: &str,
    id: &str,
    before: Option<&str>,
    updated: Option<i64>,
) -> Result<()> {
    let name = table(entity)?;
    if let Some(j) = before {
        tx.execute(&format!("INSERT INTO {name} (id,json,updated_at) VALUES (?1,?2,?3) ON CONFLICT(id) DO UPDATE SET json=?2,updated_at=?3"), params![id,j,updated.ok_or_else(|| Error::Invalid("Missing history timestamp.".into()))?])?;
    } else {
        tx.execute(&format!("DELETE FROM {name} WHERE id=?"), [id])?;
    }
    Ok(())
}

pub(super) fn current_revision<T: DeserializeOwned>(
    conn: &Connection,
    entity: &str,
    id: &str,
    revision: u32,
    get: impl FnOnce(&T) -> u32,
) -> Result<Option<T>> {
    let old: Option<String> =
        conn.query_row(&format!("SELECT json FROM {} WHERE id=?", table(entity)?), [id], |r| r.get(0)).optional()?;
    match old {
        None if revision == 0 => Ok(None),
        Some(j) => {
            let value: T = serde_json::from_str(&j)?;
            if get(&value) != revision {
                return stale();
            }
            Ok(Some(value))
        }
        _ => stale(),
    }
}

pub(super) fn stale<T>() -> Result<T> {
    Err(Error::Invalid("This record changed. Reopen it and try again.".into()))
}
