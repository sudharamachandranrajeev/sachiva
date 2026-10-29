//! Sachiva's core: everything that decides, stores and reminds, with no UI.
//! The Tauri app calls into this crate; it is tested on its own.

pub mod checker;
pub mod dashboard;
pub mod model;
pub mod priority;
pub mod recur;
pub mod reminders;
pub mod store;

pub use jiff;
