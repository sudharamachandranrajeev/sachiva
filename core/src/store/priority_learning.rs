//! Suggestions from user-made priority changes already in encrypted task history.
use super::*;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrioritySuggestion {
    pub level_id: String,
    pub examples: usize,
}

fn words(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= 4)
        .map(str::to_lowercase)
        .take(64)
        .collect()
}

impl Store {
    /// A suggestion needs at least two similar, explicitly reprioritised jobs
    /// that agree. It never changes a task or stores a second copy of its text.
    pub fn suggest_priority(&self, title: &str, exclude: Option<&str>) -> Result<Option<PrioritySuggestion>> {
        let candidate = words(title);
        if candidate.len() < 2 {
            return Ok(None);
        }
        let mut stmt = self.conn.prepare(
            "SELECT t.json FROM tasks t WHERE t.deleted_at IS NULL AND (
             EXISTS(SELECT 1 FROM priority_feedback f WHERE f.id = t.id) OR
             EXISTS(SELECT 1 FROM history h WHERE h.task_id = t.id AND h.entity = 'task' AND h.before_json IS NOT NULL
             AND coalesce(json_extract(h.before_json, '$.importance'), 'normal') != coalesce(json_extract(t.json, '$.importance'), 'normal')))
             ORDER BY t.updated_at DESC, t.id LIMIT 500"
        )?;
        let mut priority = None;
        let mut examples = 0;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        for row in rows {
            let task: Task = serde_json::from_str(&row?)?;
            if exclude == Some(task.id.as_str()) {
                continue;
            }
            let other = words(&task.title);
            if candidate.intersection(&other).take(2).count() < 2 {
                continue;
            }
            if priority.as_deref().is_some_and(|p| p != task.priority_id()) {
                return Ok(None);
            }
            priority = Some(task.priority_id().to_owned());
            examples += 1;
        }
        Ok(if examples >= 2 { priority.map(|level_id| PrioritySuggestion { level_id, examples }) } else { None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn learns_only_from_user_changes_and_never_overrides_conflicts_or_deleted_tasks() {
        let mut s = Store::memory().unwrap();
        let now: Timestamp = "2026-11-02T10:00:00Z".parse().unwrap();
        let mut a = Task::new("Review stock order", now);
        let mut b = Task::new("Review stock delivery", now);
        s.save(&a, now).unwrap();
        s.save(&b, now).unwrap();
        assert!(s.suggest_priority("Review stock request", None).unwrap().is_none());
        a.importance = crate::model::Importance::VeryHigh;
        b.importance = crate::model::Importance::VeryHigh;
        s.save(&a, now).unwrap();
        s.save(&b, now).unwrap();
        let suggestion = s.suggest_priority("Review stock request", None).unwrap().unwrap();
        assert_eq!(suggestion.level_id, "veryHigh");
        assert_eq!(suggestion.examples, 2);
        s.purge(now + SignedDuration::from_hours(24 * 31), 30).unwrap();
        assert_eq!(
            s.suggest_priority("Review stock request", None).unwrap().unwrap().examples,
            2,
            "learning survives history retention"
        );
        assert!(s.suggest_priority("Review stock request", Some(&a.id)).unwrap().is_none());
        b.importance = crate::model::Importance::Low;
        s.save(&b, now).unwrap();
        assert!(s.suggest_priority("Review stock request", None).unwrap().is_none());
        s.delete(&a.id, now).unwrap();
        assert!(s.suggest_priority("Review stock request", None).unwrap().is_none());
    }
}
