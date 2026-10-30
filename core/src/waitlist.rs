//! Waiting-list figures, entered by hand for a day, week or month.
//!
//! Numbers only: no patient appears here. A field left blank stays blank
//! ("not entered"), never 0, so a missed week shows as a gap, not a drop.
//! Each save is kept as a new row; the newest one for a period is the one
//! shown, so an edit never destroys what was there before.

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::counts;
use crate::model::{MAX_NAME, Span};

/// Wait-time band edges in weeks. Bands are 0-9, 9-18, 18-52 and over 52;
/// each list can change them.
pub const DEFAULT_BANDS: [u16; 3] = [9, 18, 52];
pub const MAX_BANDS: usize = 8;
/// Far above any real list; catches a slipped key.
pub const MAX_FIGURE: u32 = 10_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitList {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub site: String,
    #[serde(default)]
    pub clinic: String,
    /// Initials or a role, not a name.
    #[serde(default)]
    pub provider: String,
    /// Upper edges of the bands in weeks, rising; one more band holds the rest.
    pub bands: Vec<u16>,
    /// The longest wait aimed for, in weeks.
    pub target_weeks: Option<u16>,
    #[serde(default)]
    pub archived: bool,
}

impl WaitList {
    pub fn new(name: impl Into<String>) -> WaitList {
        WaitList {
            id: uuid::Uuid::now_v7().to_string(),
            name: name.into(),
            site: String::new(),
            clinic: String::new(),
            provider: String::new(),
            bands: DEFAULT_BANDS.to_vec(),
            target_weeks: None,
            archived: false,
        }
    }

    /// "0-9 weeks", "9-18 weeks", ..., "over 52 weeks".
    pub fn band_names(&self) -> Vec<String> {
        let mut v = Vec::new();
        let mut low = 0;
        for &b in &self.bands {
            v.push(format!("{low}-{b} weeks"));
            low = b;
        }
        v.push(format!("over {low} weeks"));
        v
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Give the list a name.".into());
        }
        for (what, s) in
            [("name", &self.name), ("site", &self.site), ("clinic", &self.clinic), ("provider", &self.provider)]
        {
            if s.chars().count() > MAX_NAME {
                return Err(format!("Keep the {what} under {MAX_NAME} characters."));
            }
        }
        if self.bands.is_empty() || self.bands.len() > MAX_BANDS {
            return Err(format!("Use between 1 and {MAX_BANDS} band edges."));
        }
        if self.bands[0] == 0 || self.bands.windows(2).any(|w| w[0] >= w[1]) || self.bands.iter().any(|&b| b > 520) {
            return Err("Band edges must rise, from 1 week up to 520.".into());
        }
        if self.target_weeks.is_some_and(|t| t == 0 || t > 520) {
            return Err("The target wait must be between 1 and 520 weeks.".into());
        }
        if uuid::Uuid::parse_str(&self.id).is_err() {
            return Err("This list has a broken id.".into());
        }
        Ok(())
    }
}

/// The figures for one list and one period. `None` = not entered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Figures {
    pub list_id: String,
    pub span: Span,
    /// First day of the period (a Monday for weeks, the 1st for months).
    pub start: Date,
    pub total: Option<u32>,
    pub added: Option<u32>,
    pub removed: Option<u32>,
    pub booked: Option<u32>,
    /// One per band of the list, in order.
    #[serde(default)]
    pub bands: Vec<Option<u32>>,
    pub longest_weeks: Option<u32>,
    #[serde(default)]
    pub note: String,
}

impl Figures {
    pub fn empty(list_id: &str, span: Span, start: Date, bands: usize) -> Figures {
        Figures {
            list_id: list_id.into(),
            span,
            start,
            total: None,
            added: None,
            removed: None,
            booked: None,
            bands: vec![None; bands],
            longest_weeks: None,
            note: String::new(),
        }
    }

    pub fn validate(&self, list: &WaitList) -> Result<(), String> {
        if self.list_id != list.id {
            return Err("These figures belong to another list.".into());
        }
        if !counts::is_start(self.span, self.start) {
            return Err("The period must start on its first day (Monday for weeks, the 1st for months).".into());
        }
        if self.bands.len() != list.bands.len() + 1 {
            return Err("The bands do not match the list's bands; reopen the list and try again.".into());
        }
        let all = [self.total, self.added, self.removed, self.booked, self.longest_weeks];
        if all.iter().chain(self.bands.iter()).flatten().any(|&n| n > MAX_FIGURE) {
            return Err("That number is too large; check for an extra digit.".into());
        }
        if self.longest_weeks.is_some_and(|w| w > 1040) {
            return Err("The longest wait is in weeks; 1040 is 20 years.".into());
        }
        // Bands are a split of the total: they cannot add up to more.
        if let Some(t) = self.total
            && self.bands.iter().flatten().map(|&b| u64::from(b)).sum::<u64>() > u64::from(t)
        {
            return Err("The bands add up to more than the total waiting.".into());
        }
        if self.note.chars().count() > crate::model::MAX_TITLE {
            return Err(format!("Keep the note under {} characters.", crate::model::MAX_TITLE));
        }
        Ok(())
    }

    /// Waiting longer than the list's target, worked out from the bands
    /// when the target is a band edge. `None` when it cannot be told.
    pub fn over_target(&self, list: &WaitList) -> Option<u32> {
        let t = list.target_weeks?;
        let i = list.bands.iter().position(|&b| b == t)?;
        self.bands[i + 1..].iter().copied().sum::<Option<u32>>()
    }
}

/// Adds the same field across lists. Blank if any list left it blank, so a
/// total never looks lower just because a figure is missing.
pub fn sum(values: impl IntoIterator<Item = Option<u32>>) -> Option<u32> {
    let mut n = 0u32;
    let mut any = false;
    for v in values {
        n = n.saturating_add(v?);
        any = true;
    }
    any.then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn list() -> WaitList {
        let mut l = WaitList::new("Clinic A follow-ups");
        l.target_weeks = Some(52);
        l
    }

    #[test]
    fn list_rules() {
        let mut l = list();
        assert!(l.validate().is_ok());
        assert_eq!(l.band_names(), vec!["0-9 weeks", "9-18 weeks", "18-52 weeks", "over 52 weeks"]);
        l.bands = vec![9, 9];
        assert!(l.validate().is_err());
        l.bands = vec![];
        assert!(l.validate().is_err());
    }

    #[test]
    fn figure_rules() {
        let l = list();
        let mut f = Figures::empty(&l.id, Span::Week, date(2026, 11, 2), 4);
        assert!(f.validate(&l).is_ok());
        f.start = date(2026, 11, 3);
        assert!(f.validate(&l).is_err(), "weeks start on Monday");
        f.start = date(2026, 11, 2);
        f.total = Some(100);
        f.bands = vec![Some(50), Some(30), Some(15), Some(10)];
        assert!(f.validate(&l).is_err(), "bands add up to more than the total");
        f.bands[3] = Some(5);
        assert!(f.validate(&l).is_ok());
        assert_eq!(f.over_target(&l), Some(5));
        f.bands[3] = None;
        assert_eq!(f.over_target(&l), None);
        f.bands.pop();
        assert!(f.validate(&l).is_err());
    }

    #[test]
    fn sums_keep_blanks() {
        assert_eq!(sum([Some(2), Some(3)]), Some(5));
        assert_eq!(sum([Some(2), None]), None);
        assert_eq!(sum([]), None);
    }
}
