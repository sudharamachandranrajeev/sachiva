//! Prints a dashboard built from synthetic jobs, as the app would send it to
//! the UI. Used for UI previews and to check the UI's types against serde.
//!
//!     cargo run -p sachiva-core --example demo > demo.json

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use sachiva_core::dashboard;
use sachiva_core::model::{CheckItem, Importance, Repeat, Status, Task};
use sachiva_core::reminders::Settings;

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

    let tasks = [slot, call, scan, po, list, stats, quote, blind, stock, toner.clone()];
    let tz = TimeZone::try_system().unwrap_or(TimeZone::UTC);
    let out = serde_json::json!({
        "dashboard": dashboard::build(&tasks, now, &tz),
        "done": [toner],
        "settings": Settings::default(),
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
