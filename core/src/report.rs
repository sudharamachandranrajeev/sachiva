//! Reports: one model, built here, that the screen and every export format
//! (Excel, Word, PowerPoint, PDF) draw from, so they always agree.

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};

use crate::counts::{self, Count};
use crate::model::{Kind, Span, Task};
use crate::spc::{self, Assurance, Better, Spc, Variation};
use crate::waitlist::{self, Figures, WaitList};

/// Most periods one report shows.
pub const MAX_PERIODS: usize = 120;

pub const FOOTNOTE: &str = "Made with Sachiva. Figures were entered by hand and are not official statistics. \
Charts follow the Making Data Count method (NHS England): orange = concern, blue = improvement, \
grey = no significant change.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReportKind {
    Performance,
    WaitingList,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub kind: ReportKind,
    pub span: Span,
    pub from: Date,
    pub to: Date,
    /// Activity names to include; empty = all.
    #[serde(default)]
    pub activities: Vec<String>,
    /// Waiting-list ids to include; empty = every list not archived.
    #[serde(default)]
    pub lists: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub title: String,
    /// "w/c 6 Oct 2026 to w/c 3 Nov 2026, weekly".
    pub period: String,
    pub made: String,
    pub footnote: String,
    pub summary: Vec<SummaryRow>,
    pub sections: Vec<Section>,
}

/// One line of the summary table: the icons instead of a RAG rating.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryRow {
    pub measure: String,
    pub latest: Option<f64>,
    pub target: Option<f64>,
    pub variation: Option<Variation>,
    pub assurance: Option<Assurance>,
    /// "Met in 9 of 12 weeks", or empty.
    pub met: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub title: String,
    pub intro: String,
    pub charts: Vec<Chart>,
    pub table: Table,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    /// Says what the chart shows, in words: "Calls per week".
    pub title: String,
    /// For screen readers and the alt text of exported images.
    pub alt: String,
    pub y_label: String,
    pub labels: Vec<String>,
    pub body: ChartBody,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ChartBody {
    /// A run chart with SPC limits and coloured points when there are enough values.
    Spc {
        values: Vec<Option<f64>>,
        spc: Option<Spc>,
        target: Option<f64>,
        better: Better,
    },
    Bars {
        series: Vec<Series>,
        stacked: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub name: String,
    pub values: Vec<Option<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Cell {
    Text(String),
    /// `None` = not entered; shown as a dash.
    Num(Option<f64>),
}

impl Request {
    pub fn validate(&self) -> Result<(), String> {
        if self.from > self.to {
            return Err("The start of the report comes after its end.".into());
        }
        Ok(())
    }

    fn starts(&self) -> Vec<Date> {
        counts::starts(self.span, self.from, self.to, MAX_PERIODS)
    }

    fn period_words(&self, starts: &[Date]) -> String {
        let per = match self.span {
            Span::Day => "daily",
            Span::Week => "weekly",
            Span::Month => "monthly",
        };
        match (starts.first(), starts.last()) {
            (Some(a), Some(b)) => {
                format!("{} to {}, {per}", counts::label(self.span, *a), counts::label(self.span, *b))
            }
            _ => String::new(),
        }
    }
}

fn unit_name(s: Span) -> &'static str {
    match s {
        Span::Day => "day",
        Span::Week => "week",
        Span::Month => "month",
    }
}

fn made(now: Timestamp, tz: &TimeZone) -> String {
    now.to_zoned(tz.clone()).strftime("%-d %B %Y, %H:%M").to_string()
}

/// The personal performance report: each activity's counts per period.
/// `tasks` should include trashed tasks, so their counts still have a name.
pub fn performance(req: &Request, tasks: &[Task], all_counts: &[Count], tz: &TimeZone, now: Timestamp) -> Report {
    let starts = req.starts();
    let labels: Vec<String> = starts.iter().map(|&s| counts::label(req.span, s)).collect();
    let per = unit_name(req.span);

    // Group targets and tallies by activity name.
    let mut groups: BTreeMap<String, Vec<&Task>> = BTreeMap::new();
    for t in tasks.iter().filter(|t| matches!(t.kind, Kind::Target { .. } | Kind::Tally { .. })) {
        let name = t.activity_name().to_string();
        if req.activities.is_empty() || req.activities.iter().any(|a| a.eq_ignore_ascii_case(&name)) {
            groups.entry(name).or_default().push(t);
        }
    }

    let mut summary = Vec::new();
    let mut sections = Vec::new();
    for (name, group) in &groups {
        let ids: Vec<&str> = group.iter().map(|t| t.id.as_str()).collect();
        let mine: Vec<Count> = all_counts.iter().filter(|c| ids.contains(&c.task_id.as_str())).cloned().collect();
        let first = group.iter().map(|t| t.created_at).min().map(|t| counts::local_day(t, tz));
        let totals = counts::per_period(&mine, req.span, &starts, tz);
        // Periods that ended before the activity existed are blank, not 0.
        // A period still running is blank in the chart too: a part-week
        // would read as a fall. The table shows it as "so far".
        let running = |s: Date| counts::instant(counts::next_start(req.span, s), tz) > now;
        let values: Vec<Option<f64>> = starts
            .iter()
            .zip(&totals)
            .map(|(&s, &n)| {
                let before = first.is_some_and(|f| counts::next_start(req.span, s) <= f);
                (!before && !running(s)).then_some(n as f64)
            })
            .collect();
        let target = group.iter().find_map(|t| match t.kind {
            Kind::Target { goal, every: Some(s), .. } if s == req.span => Some(f64::from(goal)),
            _ => None,
        });
        let spc = spc::analyse(&values, Better::Higher, target);
        let (met, of) = match target {
            Some(g) => {
                let shown: Vec<f64> = values.iter().flatten().copied().collect();
                (shown.iter().filter(|&&v| v >= g).count(), shown.len())
            }
            None => (0, 0),
        };
        let met_words = if target.is_some() && of > 0 { format!("Met in {met} of {of} {per}s") } else { String::new() };
        let title = format!("{name} per {per}");
        let latest = values.iter().rev().flatten().next().copied();
        summary.push(SummaryRow {
            measure: title.clone(),
            latest,
            target,
            variation: spc.as_ref().map(|s| s.variation),
            assurance: spc.as_ref().and_then(|s| s.assurance),
            met: met_words.clone(),
        });
        let mut rows = Vec::new();
        for (i, l) in labels.iter().enumerate() {
            if running(starts[i]) {
                let so_far = Cell::Num(Some(totals[i] as f64));
                rows.push(vec![
                    Cell::Text(format!("{l} (so far)")),
                    so_far,
                    Cell::Num(target),
                    Cell::Text(String::new()),
                ]);
                continue;
            }
            let v = values[i];
            let hit = match (v, target) {
                (Some(v), Some(g)) => Cell::Text(if v >= g { "Yes".into() } else { "No".into() }),
                _ => Cell::Text(String::new()),
            };
            rows.push(vec![Cell::Text(l.clone()), Cell::Num(v), Cell::Num(target), hit]);
        }
        let alt = chart_alt(&title, &labels, &values, spc.as_ref(), target);
        sections.push(Section {
            title: name.clone(),
            intro: if met_words.is_empty() { String::new() } else { format!("{met_words}.") },
            charts: vec![Chart {
                title,
                alt,
                y_label: name.clone(),
                labels: labels.clone(),
                body: ChartBody::Spc { values, spc, target, better: Better::Higher },
            }],
            table: Table { columns: vec!["Period".into(), name.clone(), "Target".into(), "Target met".into()], rows },
        });
    }
    Report {
        title: "Personal performance report".into(),
        period: req.period_words(&starts),
        made: made(now, tz),
        footnote: FOOTNOTE.into(),
        summary,
        sections,
    }
}

/// The waiting-list report: figures per list and, with several lists, their total.
pub fn waiting(req: &Request, lists: &[WaitList], figures: &[Figures], tz: &TimeZone, now: Timestamp) -> Report {
    let starts = req.starts();
    let labels: Vec<String> = starts.iter().map(|&s| counts::label(req.span, s)).collect();
    let chosen: Vec<&WaitList> =
        lists.iter().filter(|l| if req.lists.is_empty() { !l.archived } else { req.lists.contains(&l.id) }).collect();

    // Figures for each chosen list, one slot per period (None = not entered).
    let grid: Vec<Vec<Option<&Figures>>> = chosen
        .iter()
        .map(|l| {
            starts
                .iter()
                .map(|s| figures.iter().find(|f| f.list_id == l.id && f.span == req.span && f.start == *s))
                .collect()
        })
        .collect();

    let mut summary = Vec::new();
    let mut sections = Vec::new();
    if chosen.len() > 1 {
        let same_bands = chosen.windows(2).all(|w| w[0].bands == w[1].bands);
        let field = |get: &dyn Fn(&Figures) -> Option<u32>| -> Vec<Option<u32>> {
            (0..starts.len()).map(|p| waitlist::sum(grid.iter().map(|row| row[p].and_then(get)))).collect()
        };
        let combined = Combined {
            name: "All selected lists".into(),
            total: field(&|f| f.total),
            added: field(&|f| f.added),
            removed: field(&|f| f.removed),
            booked: field(&|f| f.booked),
            bands: if same_bands {
                (0..=chosen[0].bands.len()).map(|b| field(&|f: &Figures| f.bands.get(b).copied().flatten())).collect()
            } else {
                Vec::new()
            },
            band_names: if same_bands { chosen[0].band_names() } else { Vec::new() },
            over: None,
            longest: (0..starts.len())
                .map(|p| {
                    let v: Option<Vec<u32>> = grid.iter().map(|row| row[p].and_then(|f| f.longest_weeks)).collect();
                    v.and_then(|v| v.into_iter().max())
                })
                .collect(),
            target_weeks: None,
        };
        list_section(&combined, &labels, req.span, &mut summary, &mut sections);
    }
    for (l, row) in chosen.iter().zip(&grid) {
        let pick = |get: &dyn Fn(&Figures) -> Option<u32>| -> Vec<Option<u32>> {
            row.iter().map(|f| f.and_then(get)).collect()
        };
        let c = Combined {
            name: list_name(l),
            total: pick(&|f| f.total),
            added: pick(&|f| f.added),
            removed: pick(&|f| f.removed),
            booked: pick(&|f| f.booked),
            bands: (0..=l.bands.len()).map(|b| pick(&|f: &Figures| f.bands.get(b).copied().flatten())).collect(),
            band_names: l.band_names(),
            over: l.target_weeks.and_then(|t| {
                l.bands.contains(&t).then(|| (t, row.iter().map(|f| f.and_then(|f| f.over_target(l))).collect()))
            }),
            longest: pick(&|f| f.longest_weeks),
            target_weeks: l.target_weeks,
        };
        list_section(&c, &labels, req.span, &mut summary, &mut sections);
    }
    Report {
        title: "Waiting list report".into(),
        period: req.period_words(&starts),
        made: made(now, tz),
        footnote: FOOTNOTE.into(),
        summary,
        sections,
    }
}

fn list_name(l: &WaitList) -> String {
    let parts: Vec<&str> = [l.site.as_str(), l.clinic.as_str(), l.provider.as_str()]
        .into_iter()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() { l.name.trim().to_string() } else { format!("{} ({})", l.name.trim(), parts.join(", ")) }
}

/// One list's figures (or a total of several), period by period.
struct Combined {
    name: String,
    total: Vec<Option<u32>>,
    added: Vec<Option<u32>>,
    removed: Vec<Option<u32>>,
    booked: Vec<Option<u32>>,
    bands: Vec<Vec<Option<u32>>>,
    band_names: Vec<String>,
    /// (target weeks, number waiting longer) when the target is a band edge.
    over: Option<(u16, Vec<Option<u32>>)>,
    longest: Vec<Option<u32>>,
    target_weeks: Option<u16>,
}

fn f64s(v: &[Option<u32>]) -> Vec<Option<f64>> {
    v.iter().map(|x| x.map(f64::from)).collect()
}

fn list_section(
    c: &Combined,
    labels: &[String],
    span: Span,
    summary: &mut Vec<SummaryRow>,
    sections: &mut Vec<Section>,
) {
    let per = unit_name(span);
    let mut charts = Vec::new();
    let mut spc_chart = |title: String, y: &str, values: Vec<Option<f64>>, target: Option<f64>| {
        let spc = spc::analyse(&values, Better::Lower, target);
        summary.push(SummaryRow {
            measure: format!("{}: {}", c.name, lower_first(&title)),
            latest: values.iter().rev().flatten().next().copied(),
            target,
            variation: spc.as_ref().map(|s| s.variation),
            assurance: spc.as_ref().and_then(|s| s.assurance),
            met: String::new(),
        });
        let alt = chart_alt(&format!("{}: {}", c.name, lower_first(&title)), labels, &values, spc.as_ref(), target);
        charts.push(Chart {
            title,
            alt,
            y_label: y.into(),
            labels: labels.to_vec(),
            body: ChartBody::Spc { values, spc, target, better: Better::Lower },
        });
    };
    spc_chart(format!("Total waiting, each {per}"), "People waiting", f64s(&c.total), None);
    if let Some((weeks, over)) = &c.over {
        spc_chart(format!("Waiting over {weeks} weeks"), "People waiting", f64s(over), None);
    }
    spc_chart("Longest wait in weeks".into(), "Weeks", f64s(&c.longest), c.target_weeks.map(f64::from));

    let movement = [("Added", &c.added), ("Removed", &c.removed), ("Booked from the list", &c.booked)];
    if movement.iter().any(|(_, v)| v.iter().any(Option::is_some)) {
        let series: Vec<Series> = movement.iter().map(|(n, v)| Series { name: (*n).into(), values: f64s(v) }).collect();
        charts.push(Chart {
            title: format!("Added, removed and booked, each {per}"),
            alt: bars_alt(&format!("{}: added, removed and booked", c.name), labels, &series),
            y_label: "People".into(),
            labels: labels.to_vec(),
            body: ChartBody::Bars { series, stacked: false },
        });
    }
    if !c.bands.is_empty() && c.bands.iter().any(|b| b.iter().any(Option::is_some)) {
        let series: Vec<Series> =
            c.band_names.iter().zip(&c.bands).map(|(n, v)| Series { name: n.clone(), values: f64s(v) }).collect();
        charts.push(Chart {
            title: "Waiting by how long they have waited".into(),
            alt: bars_alt(&format!("{}: waiting by band", c.name), labels, &series),
            y_label: "People waiting".into(),
            labels: labels.to_vec(),
            body: ChartBody::Bars { series, stacked: true },
        });
    }

    let mut columns =
        vec!["Period".to_string(), "Total waiting".into(), "Added".into(), "Removed".into(), "Booked".into()];
    columns.extend(c.band_names.iter().cloned());
    columns.push("Longest wait (weeks)".into());
    let rows = labels
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let mut r = vec![
                Cell::Text(l.clone()),
                Cell::Num(c.total[i].map(f64::from)),
                Cell::Num(c.added[i].map(f64::from)),
                Cell::Num(c.removed[i].map(f64::from)),
                Cell::Num(c.booked[i].map(f64::from)),
            ];
            r.extend(c.bands.iter().map(|b| Cell::Num(b[i].map(f64::from))));
            r.push(Cell::Num(c.longest[i].map(f64::from)));
            r
        })
        .collect();
    let intro = match c.target_weeks {
        Some(t) => format!("Target: nobody waits longer than {t} weeks. Blank cells were not entered."),
        None => "Blank cells were not entered.".into(),
    };
    sections.push(Section { title: c.name.clone(), intro, charts, table: Table { columns, rows } });
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn num(v: f64) -> String {
    if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.1}") }
}

fn chart_alt(title: &str, labels: &[String], values: &[Option<f64>], spc: Option<&Spc>, target: Option<f64>) -> String {
    let shown: Vec<(usize, f64)> = values.iter().enumerate().filter_map(|(i, v)| v.map(|v| (i, v))).collect();
    let Some(&(li, lv)) = shown.last() else {
        return format!("{title}: no figures entered for this period.");
    };
    let mut s = format!("{title}, {} periods. Latest ({}): {}.", labels.len(), labels[li], num(lv));
    if let (Some(lo), Some(hi)) =
        (shown.iter().map(|p| p.1).reduce(f64::min), shown.iter().map(|p| p.1).reduce(f64::max))
    {
        s.push_str(&format!(" Lowest {}, highest {}.", num(lo), num(hi)));
    }
    if let Some(t) = target {
        s.push_str(&format!(" Target {}.", num(t)));
    }
    if let Some(spc) = spc {
        s.push_str(&format!(" Average {}. {}.", num(spc.mean), spc.variation.words()));
        if let Some(a) = spc.assurance {
            s.push_str(&format!(" {}.", a.words()));
        }
    }
    s
}

fn bars_alt(title: &str, labels: &[String], series: &[Series]) -> String {
    let last = labels.len().checked_sub(1);
    let latest: Vec<String> = series
        .iter()
        .filter_map(|s| last.and_then(|i| s.values[i]).map(|v| format!("{} {}", s.name.to_lowercase(), num(v))))
        .collect();
    match (last, latest.is_empty()) {
        (Some(i), false) => {
            format!("{title}, {} periods. Latest ({}): {}.", labels.len(), labels[i], latest.join(", "))
        }
        _ => format!("{title}, {} periods.", labels.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn tz() -> TimeZone {
        TimeZone::get("Europe/London").unwrap()
    }

    #[test]
    fn performance_report() {
        let now: Timestamp = "2026-11-10T12:00:00Z".parse().unwrap();
        let mut calls = Task::new("Ring back", "2026-10-20T09:00:00Z".parse().unwrap());
        calls.activity = "Calls".into();
        calls.kind = Kind::Target { goal: 10, unit: "calls".into(), every: Some(Span::Week) };
        let c = |at: &str, d: i64| Count {
            id: 0,
            task_id: calls.id.clone(),
            at: at.parse().unwrap(),
            delta: d,
            note: String::new(),
        };
        let counts = [c("2026-10-21T10:00:00Z", 12), c("2026-10-28T10:00:00Z", 8), c("2026-11-04T10:00:00Z", 10)];
        let req = Request {
            kind: ReportKind::Performance,
            span: Span::Week,
            from: date(2026, 10, 12),
            to: date(2026, 11, 8),
            activities: vec![],
            lists: vec![],
        };
        let r = performance(&req, &[calls.clone()], &counts, &tz(), now);
        assert_eq!(r.sections.len(), 1);
        let ChartBody::Spc { values, target, .. } = &r.sections[0].charts[0].body else { panic!() };
        // The week of 12 Oct ended before the task existed: blank, not 0.
        assert_eq!(values, &vec![None, Some(12.0), Some(8.0), Some(10.0)]);
        assert_eq!(*target, Some(10.0));
        assert_eq!(r.summary[0].met, "Met in 2 of 3 weeks");
        assert!(r.period.starts_with("w/c 12 Oct 2026 to w/c 2 Nov 2026"));
        assert!(r.sections[0].charts[0].alt.contains("Latest (w/c 2 Nov 2026): 10"));

        // The week still running is not charted (a part-week would look
        // like a fall) but shows in the table as "so far".
        let late = [&counts[..], &[c("2026-11-10T09:00:00Z", 3)]].concat();
        let req = Request { to: date(2026, 11, 10), ..req };
        let r = performance(&req, &[calls], &late, &tz(), now);
        let ChartBody::Spc { values, .. } = &r.sections[0].charts[0].body else { panic!() };
        assert_eq!(values.last(), Some(&None));
        assert_eq!(r.summary[0].latest, Some(10.0));
        assert_eq!(r.summary[0].met, "Met in 2 of 3 weeks");
        let last = r.sections[0].table.rows.last().unwrap();
        assert_eq!(last[0], Cell::Text("w/c 9 Nov 2026 (so far)".into()));
        assert_eq!(last[1], Cell::Num(Some(3.0)));
    }

    #[test]
    fn waiting_report_sums_lists_and_keeps_blanks() {
        let now: Timestamp = "2026-11-10T12:00:00Z".parse().unwrap();
        let mut a = WaitList::new("Follow-ups");
        a.site = "Site A".into();
        a.target_weeks = Some(52);
        let b = WaitList::new("New patients");
        let mut fa = Figures::empty(&a.id, Span::Month, date(2026, 10, 1), 4);
        fa.total = Some(100);
        fa.bands = vec![Some(40), Some(30), Some(20), Some(10)];
        fa.longest_weeks = Some(60);
        let mut fb = Figures::empty(&b.id, Span::Month, date(2026, 10, 1), 4);
        fb.total = Some(50);
        fb.longest_weeks = Some(30);
        let mut fa2 = Figures::empty(&a.id, Span::Month, date(2026, 9, 1), 4);
        fa2.total = Some(110);
        let req = Request {
            kind: ReportKind::WaitingList,
            span: Span::Month,
            from: date(2026, 9, 1),
            to: date(2026, 10, 31),
            activities: vec![],
            lists: vec![],
        };
        let r = waiting(&req, &[a.clone(), b], &[fa, fb, fa2], &tz(), now);
        assert_eq!(r.sections.len(), 3);
        let all = &r.sections[0];
        assert_eq!(all.title, "All selected lists");
        let ChartBody::Spc { values, .. } = &all.charts[0].body else { panic!() };
        // September: list B blank, so the total is blank, not 110.
        assert_eq!(values, &vec![None, Some(150.0)]);
        assert_eq!(r.sections[1].title, "Follow-ups (Site A)");
        // Over-52-week chart from the bands, and longest wait against its target.
        assert!(r.sections[1].charts.iter().any(|c| c.title == "Waiting over 52 weeks"));
        assert!(
            r.summary
                .iter()
                .any(|s| s.measure == "Follow-ups (Site A): longest wait in weeks" && s.target == Some(52.0))
        );
        assert_eq!(all.table.rows[1][1], Cell::Num(Some(150.0)));
    }
}
