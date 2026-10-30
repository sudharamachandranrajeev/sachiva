//! Prints a dashboard, counters, waiting lists and report previews built
//! from synthetic data, as the app would send them to the UI. Used for UI
//! previews and to check the UI's types against serde.
//!
//!     cargo run -p sachiva-export --example demo > demo.json

use sachiva_core::counts::{self, Count};
use sachiva_core::dashboard;
use sachiva_core::jiff::tz::TimeZone;
use sachiva_core::jiff::{SignedDuration, Timestamp, ToSpan};
use sachiva_core::model::{CheckItem, Importance, Kind, Repeat, Span, Status, Task};
use sachiva_core::reminders::Settings;
use sachiva_core::report::{self, Report, ReportKind, Request};
use sachiva_core::waitlist::{Figures, WaitList};

/// The same fields the app's preview_report command sends.
fn preview(report: Report) -> serde_json::Value {
    let summary = sachiva_export::summary_rows(&report);
    let charts: Vec<Vec<String>> =
        report.sections.iter().map(|s| s.charts.iter().map(sachiva_export::chart::svg).collect()).collect();
    serde_json::json!({ "report": report, "summary": summary, "charts": charts })
}

/// A repeatable wobble for synthetic figures.
fn wobble(i: i64, spread: i64) -> i64 {
    (i * 7919 + 13) % (2 * spread + 1) - spread
}

fn main() {
    let now = Timestamp::now();
    let h = |n: i64| now + SignedDuration::from_mins(n * 60);
    let made = h(-72);
    let job = |title: &str| Task::new(title, made);

    let mut slot = job("Offer 14:30 cancelled slot, Site B diabetes clinic");
    slot.appointment = Some(h(3));
    slot.effort_min = Some(30);
    slot.importance = Importance::High;
    slot.site = "Site B".into();

    let mut call = job("Return call: REF-000214 rebook");
    call.due = Some(h(-20));
    call.site = "Site A".into();

    let mut scan = job("Scan referral letters (batch 12)");
    scan.due = Some(h(-50));
    scan.effort_min = Some(45);

    let mut po = job("Chase PO-0419 dressings delivery");
    po.due = Some(h(2));
    po.site = "Site A".into();
    po.tags = vec!["orders".into()];

    let mut list = job("Confirm Dr K clinic list, Thu");
    list.due = Some(h(5));
    list.site = "Site B".into();
    list.pinned = true;

    let mut stats = job("Month-end stats for both sites");
    stats.due = Some(h(30));
    stats.effort_min = Some(120);
    stats.repeat = Some(Repeat::Monthly { every: 1, day: 28 });
    stats.checklist = (1..=9).map(|i| CheckItem { text: format!("Step {i}"), done: i <= 4 }).collect();

    let mut quote = job("Supplier quote: exam couch");
    quote.status = Status::Waiting { on: "supplier".into(), chase: Some(h(-1)) };
    quote.tags = vec!["orders".into()];

    let mut blind = job("Estates: Room 3 blind repair");
    blind.status = Status::Waiting { on: "estates".into(), chase: Some(h(70)) };

    let mut stock = job("Weekly stock check, Site A store room");
    stock.due = Some(h(96));
    stock.site = "Site A".into();
    stock.repeat = Some(Repeat::Weekly { every: 1, days: vec![] });

    let mut toner = job("Order printer toner");
    toner.status = Status::Done { at: h(-2) };

    let mut triage = job("Triage new referrals");
    triage.kind = Kind::Target { goal: 40, unit: "referrals".into(), every: Some(Span::Week) };
    triage.activity = "Referral triage".into();
    let mut calls = job("Patient calls handled");
    calls.kind = Kind::Tally { unit: "calls".into() };
    calls.activity = "Phone calls".into();
    let mut audit = job("Notes audit sample");
    audit.kind = Kind::Target { goal: 60, unit: "notes".into(), every: None };
    audit.due = Some(h(24 * 9));
    audit.activity = "Notes audit".into();
    for t in [&mut triage, &mut calls] {
        t.created_at = now - SignedDuration::from_hours(24 * 150);
    }
    audit.created_at = now - SignedDuration::from_hours(24 * 12);

    let tz = TimeZone::try_system().unwrap_or(TimeZone::UTC);
    let today = counts::local_day(now, &tz);

    // Synthetic counts: one entry per working day over 20 weeks.
    let mut all = Vec::new();
    for d in 0..140 {
        let day = today - d.days();
        if day.weekday().to_monday_one_offset() > 5 {
            continue;
        }
        let at = counts::instant(day, &tz) + SignedDuration::from_hours(15);
        if at > now {
            continue;
        }
        let lift = if d < 40 { 2 } else { 0 };
        for (t, base, spread) in [(&triage, 7, 3), (&calls, 18, 6)] {
            let delta = base + lift + wobble(d, spread);
            all.push(Count { id: all.len() as i64 + 1, task_id: t.id.clone(), at, delta, note: String::new() });
        }
        if d < 12 {
            all.push(Count { id: all.len() as i64 + 1, task_id: audit.id.clone(), at, delta: 3, note: String::new() });
        }
    }
    let of = |t: &Task| all.iter().filter(|c| c.task_id == t.id).cloned().collect::<Vec<_>>();
    let counters: Vec<_> = [&triage, &audit, &calls]
        .into_iter()
        .map(|t| serde_json::json!({ "task": t, "progress": counts::progress(t, &of(t), now, &tz) }))
        .collect();

    // Two synthetic waiting lists with weekly figures, one week not entered.
    let mut follow = WaitList::new("Follow-ups, Clinic A");
    follow.site = "Site A".into();
    follow.clinic = "Clinic A".into();
    follow.provider = "Consultant 1".into();
    follow.target_weeks = Some(18);
    let mut fresh = WaitList::new("New referrals, Clinic B");
    fresh.site = "Site B".into();
    fresh.clinic = "Clinic B".into();
    let week = counts::period_start(Span::Week, today);
    let mut figures = Vec::new();
    for w in 0..20i64 {
        let start = week - (7 * (19 - w)).days();
        for (l, base, step) in [(&follow, 220, -3), (&fresh, 140, 2)] {
            if l.id == fresh.id && w == 12 {
                continue;
            }
            let total = (base + step * w + wobble(w, 6)) as u32;
            let mut f = Figures::empty(&l.id, Span::Week, start, l.bands.len() + 1);
            f.total = Some(total);
            f.added = Some((24 + wobble(w + 3, 5)) as u32);
            f.removed = Some((22 + wobble(w + 5, 5)) as u32);
            f.booked = Some((15 + wobble(w + 1, 4)) as u32);
            let (a, b, c) = (total * 45 / 100, total * 30 / 100, total * 18 / 100);
            f.bands = vec![Some(a), Some(b), Some(c), Some(total - a - b - c)];
            f.longest_weeks = Some((60 - w / 2) as u32);
            figures.push(f);
        }
    }
    let lists = [follow, fresh];
    let req = |kind| Request {
        kind,
        span: Span::Week,
        from: week - (7 * 19).days(),
        to: today,
        activities: vec![],
        lists: vec![],
    };
    let mut tasks = vec![slot, call, scan, po, list, stats, quote, blind, stock, toner.clone()];
    tasks.extend([triage, calls, audit]);
    let out = serde_json::json!({
        "dashboard": dashboard::build(&tasks, now, &tz, Settings::default().urgent_score),
        "done": [toner],
        "settings": Settings::default(),
        "counters": counters,
        "waitlists": lists,
        "figures": figures.last(),
        "performance": preview(report::performance(&req(ReportKind::Performance), &tasks, &all, &tz, now)),
        "waiting": preview(report::waiting(&req(ReportKind::WaitingList), &lists, &figures, &tz, now)),
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
