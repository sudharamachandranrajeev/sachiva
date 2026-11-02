//! User-ordered priority levels; changes share the normal transactional undo.
use super::*;
use crate::priority::PriorityConfig;

pub(super) fn load_config(conn: &Connection) -> Result<PriorityConfig> {
    let json: Option<String> =
        conn.query_row("SELECT json FROM priority_config WHERE id = 'levels'", [], |r| r.get(0)).optional()?;
    match json {
        Some(j) => Ok(serde_json::from_str(&j)?),
        None => Ok(PriorityConfig::default()),
    }
}

impl Store {
    pub fn priority_levels(&self) -> Result<PriorityConfig> {
        load_config(&self.conn)
    }
    pub fn save_priority_levels(&mut self, mut config: PriorityConfig, now: Timestamp) -> Result<PriorityConfig> {
        self.change(|tx, group| {
            let old = load_config(tx)?;
            if old.revision != config.revision {
                return Err(Error::Invalid("Priority levels changed. Reopen them and try again.".into()));
            }
            if config.levels.len() > 30 || config.levels.len() < old.levels.len() {
                return Err(Error::Invalid("Keep all existing levels; at most 30 are allowed.".into()));
            }
            let mut ids = std::collections::HashSet::new();
            let mut labels = std::collections::HashSet::new();
            for p in &mut config.levels {
                p.label = p.label.trim().to_owned();
                if p.label.is_empty()
                    || p.label.chars().count() > 60
                    || !ids.insert(p.id.clone())
                    || !labels.insert(p.label.to_lowercase())
                {
                    return Err(Error::Invalid("Use unique priority names of 1 to 60 characters.".into()));
                }
                if !old.levels.iter().any(|o| o.id == p.id) && uuid::Uuid::parse_str(&p.id).is_err() {
                    return Err(Error::Invalid("A new priority needs a valid ID.".into()));
                }
            }
            if old.levels.iter().any(|p| !ids.contains(&p.id)) {
                return Err(Error::Invalid("Existing priorities cannot be removed.".into()));
            }
            config.revision = config
                .revision
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("Priority revision limit reached.".into()))?;
            records::put(tx, group, "priority_config", "levels", &config, "change priorities", now)?;
            Ok(config)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dashboard, jiff::tz::TimeZone, priority::PriorityLevel};
    fn now() -> Timestamp {
        "2026-11-02T10:00:00Z".parse().unwrap()
    }

    #[test]
    fn custom_levels_rank_reorder_and_survive_backup_and_undo() {
        let mut s = Store::memory().unwrap();
        let mut config = s.priority_levels().unwrap();
        let custom = uuid::Uuid::now_v7().to_string();
        config.levels.push(PriorityLevel { id: custom.clone(), label: "Critical".into() });
        let mut config = s.save_priority_levels(config, now()).unwrap();
        let mut critical = Task::new("Critical job", now());
        critical.priority_level = Some(custom.clone());
        let mut high = Task::new("High job", now());
        high.importance = crate::model::Importance::High;
        s.save(&critical, now()).unwrap();
        s.save(&high, now()).unwrap();
        let ordered = dashboard::build_with_levels(&s.tasks().unwrap(), now(), &TimeZone::UTC, 85, &config);
        assert_eq!(ordered.now.unwrap().task.title, "Critical job");
        let from = config.levels.iter().position(|p| p.id == custom).unwrap();
        let item = config.levels.remove(from);
        config.levels.insert(0, item);
        config = s.save_priority_levels(config, now()).unwrap();
        let ordered = dashboard::build_with_levels(&s.tasks().unwrap(), now(), &TimeZone::UTC, 85, &config);
        assert_eq!(ordered.now.unwrap().task.title, "High job");
        let mut stale = config.clone();
        stale.revision -= 1;
        assert!(s.save_priority_levels(stale, now()).is_err());
        let mut bad = config.clone();
        bad.levels.retain(|p| p.id != custom);
        assert!(s.save_priority_levels(bad, now()).is_err());
        s.undo().unwrap();
        let restored = s.priority_levels().unwrap();
        assert_eq!(restored.levels.last().unwrap().id, custom);
        assert_eq!(s.task(&critical.id).unwrap().priority_id(), custom);
    }

    #[test]
    fn custom_priority_and_learning_survive_encrypted_backup() {
        let dir = tempfile::tempdir().unwrap();
        let key = [19; 32];
        let mut s = Store::open(&dir.path().join("main.db"), &key).unwrap();
        let mut config = s.priority_levels().unwrap();
        let id = uuid::Uuid::now_v7().to_string();
        config.levels.push(PriorityLevel { id: id.clone(), label: "Critical".into() });
        s.save_priority_levels(config, now()).unwrap();
        for title in ["Review stock order", "Review stock delivery"] {
            let mut task = Task::new(title, now());
            s.save(&task, now()).unwrap();
            task.priority_level = Some(id.clone());
            s.save(&task, now()).unwrap();
        }
        let backup = s.daily_backup(&dir.path().join("backups"), &key, now(), 14).unwrap().unwrap();
        let restored = Store::open(&backup, &key).unwrap();
        assert_eq!(restored.priority_levels().unwrap().levels.last().unwrap().label, "Critical");
        assert_eq!(restored.suggest_priority("Review stock request", None).unwrap().unwrap().level_id, id);
    }

    #[test]
    fn rejects_unknown_custom_levels_and_duplicate_names() {
        let mut s = Store::memory().unwrap();
        let mut task = Task::new("A task", now());
        task.priority_level = Some(uuid::Uuid::now_v7().to_string());
        assert!(s.save(&task, now()).is_err());
        let mut config = s.priority_levels().unwrap();
        config.levels.push(PriorityLevel { id: uuid::Uuid::now_v7().to_string(), label: "HIGH".into() });
        assert!(s.save_priority_levels(config, now()).is_err());
    }
}
