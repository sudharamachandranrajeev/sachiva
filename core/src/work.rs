//! Clinic operations belong to tasks: one checklist, deadline and reminder
//! schedule, with typed progress rather than parallel copies of the job.
use crate::model::{Kind, MAX_NAME, Status, Task};
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkflowKind {
    Session,
    Procurement,
    Delivery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Request,
    Quote,
    Ordered,
    Partial,
    Received,
    Invoice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Workflow {
    Session {
        list_id: String,
        minutes: u16,
        cancelled: bool,
    },
    Procurement {
        stage: Stage,
        supplier: String,
        quote_ref: String,
        po_ref: String,
        quantity: u32,
        received: u32,
        expected: Option<Timestamp>,
        invoice_ref: String,
    },
    Delivery {
        window_end: Option<Timestamp>,
        receiver: String,
        confirmed_at: Option<Timestamp>,
    },
}

impl Workflow {
    pub fn kind(&self) -> WorkflowKind {
        match self {
            Self::Session { .. } => WorkflowKind::Session,
            Self::Procurement { .. } => WorkflowKind::Procurement,
            Self::Delivery { .. } => WorkflowKind::Delivery,
        }
    }
    pub fn validate(&self, task: &Task) -> Result<(), String> {
        if task.repeat.is_some() || task.is_counter() || matches!(task.status, Status::Waiting { .. }) {
            return Err("Workflow jobs use their own progress; use a template for the next job.".into());
        }
        let short = |s: &str| {
            if s.chars().count() > MAX_NAME {
                Err(format!("Keep workflow references under {MAX_NAME} characters."))
            } else {
                Ok(())
            }
        };
        match self {
            Self::Session { list_id, minutes, cancelled } => {
                if uuid::Uuid::parse_str(list_id).is_err() {
                    return Err("Choose a clinic list for this session.".into());
                }
                if !(1..=1440).contains(minutes) {
                    return Err("Session length must be 1 to 1440 minutes.".into());
                }
                let start = task.appointment.ok_or("Set the clinic session time.")?;
                start
                    .checked_add(SignedDuration::from_mins(i64::from(*minutes)))
                    .map_err(|_| "Session ends outside the supported date range.")?;
                if *cancelled && !task.is_done() {
                    return Err("A cancelled session must be closed.".into());
                }
            }
            Self::Procurement { stage, supplier, quote_ref, po_ref, quantity, received, expected: _, invoice_ref } => {
                for s in [supplier, quote_ref, po_ref, invoice_ref] {
                    short(s)?;
                }
                if !(1..=1_000_000).contains(quantity) || received > quantity {
                    return Err("Enter an ordered quantity from 1 to 1000000; received cannot exceed it.".into());
                }
                if *stage >= Stage::Quote && quote_ref.trim().is_empty() {
                    return Err("Record the quote reference (or verbal quote note).".into());
                }
                if *stage >= Stage::Ordered && po_ref.trim().is_empty() {
                    return Err("Record the order or PO reference.".into());
                }
                match stage {
                    Stage::Request | Stage::Quote | Stage::Ordered if *received != 0 => {
                        return Err("Use Partial or Received when goods have arrived.".into());
                    }
                    Stage::Partial if *received == 0 || received == quantity => {
                        return Err("Partial receipt must be greater than zero and below the ordered quantity.".into());
                    }
                    Stage::Received | Stage::Invoice if received != quantity => {
                        return Err(
                            "All ordered items must be received before closing receipt or recording the invoice."
                                .into(),
                        );
                    }
                    _ => {}
                }
                if *stage == Stage::Invoice && invoice_ref.trim().is_empty() {
                    return Err("Record the invoice reference.".into());
                }
                if task.is_done() && *stage != Stage::Invoice {
                    return Err("Record receipt and the invoice before completing procurement.".into());
                }
            }
            Self::Delivery { window_end, receiver, confirmed_at } => {
                short(receiver)?;
                let start = task.appointment.ok_or("Set the start of the delivery window.")?;
                if window_end.is_none_or(|end| end <= start) {
                    return Err("The delivery window must end after it starts.".into());
                }
                if task.site.trim().is_empty() || receiver.trim().is_empty() {
                    return Err("Set the delivery site and receiving role or initials.".into());
                }
                if task.is_done() && confirmed_at.is_none() {
                    return Err("Confirm delivery before completing this job.".into());
                }
                if confirmed_at.is_some() && !task.is_done() {
                    return Err("A confirmed delivery must be completed.".into());
                }
            }
        }
        Ok(())
    }

    pub fn check_change(old: &Task, new: &Task) -> Result<(), String> {
        match (&old.workflow, &new.workflow) {
            (Some(a), Some(b)) if a.kind() == b.kind() => {
                if let (
                    Self::Procurement { stage: a, received: ar, .. },
                    Self::Procurement { stage: b, received: br, .. },
                ) = (a, b)
                    && (b < a || br < ar)
                {
                    return Err("Use Undo to reverse procurement progress or receipt quantities.".into());
                }
                if let (Self::Session { list_id: a, .. }, Self::Session { list_id: b, .. }) = (a, b)
                    && a != b
                {
                    return Err("Create another session to change its clinic identity.".into());
                }
                Ok(())
            }
            (Some(_), _) => Err("A saved workflow cannot change type; create another job.".into()),
            _ => Ok(()),
        }
    }
}

pub fn new_task(kind: WorkflowKind, list_id: Option<String>, now: Timestamp) -> Task {
    let mut task = Task::new("", now);
    task.kind = Kind::Appointment;
    task.workflow = Some(match kind {
        WorkflowKind::Session => {
            Workflow::Session { list_id: list_id.unwrap_or_default(), minutes: 60, cancelled: false }
        }
        WorkflowKind::Procurement => {
            task.kind = Kind::Deadline;
            Workflow::Procurement {
                stage: Stage::Request,
                supplier: String::new(),
                quote_ref: String::new(),
                po_ref: String::new(),
                quantity: 1,
                received: 0,
                expected: None,
                invoice_ref: String::new(),
            }
        }
        WorkflowKind::Delivery => Workflow::Delivery { window_end: None, receiver: String::new(), confirmed_at: None },
    });
    task
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: String,
    pub name: String,
    pub task: Task,
    pub archived: bool,
    pub revision: u32,
}

impl Template {
    pub fn instantiate(&self, now: Timestamp) -> Task {
        let mut t = self.task.clone();
        t.id = uuid::Uuid::now_v7().to_string();
        t.created_at = now;
        t.updated_at = now;
        t.status = Status::Open;
        t.due = None;
        t.appointment = None;
        t.snoozed_until = None;
        t.pinned = false;
        for item in &mut t.checklist {
            item.done = false;
        }
        match &mut t.workflow {
            Some(Workflow::Session { cancelled, .. }) => *cancelled = false,
            Some(Workflow::Procurement { stage, quote_ref, po_ref, received, expected, invoice_ref, .. }) => {
                *stage = Stage::Request;
                quote_ref.clear();
                po_ref.clear();
                *received = 0;
                *expected = None;
                invoice_ref.clear();
            }
            Some(Workflow::Delivery { confirmed_at, window_end, .. }) => {
                *confirmed_at = None;
                *window_end = None;
            }
            None => {}
        }
        t
    }
}
