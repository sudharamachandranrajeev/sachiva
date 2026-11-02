//! Sachiva's core: everything that decides, stores and reminds, with no UI.
//! The Tauri app calls into this crate; it is tested on its own.

pub mod checker;
pub mod clinic;
pub mod counts;
pub mod dashboard;
pub mod model;
pub mod priority;
pub mod recur;
pub mod reminders;
pub mod report;
pub mod spc;
pub mod store;
pub mod waitlist;
pub mod work;

pub use jiff;
