# Phase 4 acceptance audit

Scope: the clinic-workflow phase in the project plan. Release: 0.3.0 (alpha).

| Planned behavior | Implementation and proof |
| --- | --- |
| Sites, clinics, providers | Existing waiting lists hold the combination; identities are protected once clinic records use them. |
| Clinic sessions | Typed task workflow with list, start, duration, checklist, completion/cancellation, reminders and undo. |
| Cancellation → refill | Cancelled slot form; same-list matching by type, duration and local preferred days/window; target-date ranking; calls; booking confirmation; filled/lost outcomes. Device workflow and core tests pass. |
| Reminders for open slots | Reuses task rules; Android schedules heads-up/due/renewal and cancels them on close/undo. Device scheduling/cancellation verified. |
| Refill figures | Dashboard open/filled/lost, closed-slot refill percentage and mean time to fill. Device verification and hand-worked core tests. |
| Procurement | Request/quote/order, expected arrival and chase dates, partial/full receipts, invoice reference. Validation blocks over-receipt, backwards progress and premature completion. |
| Deliveries | Site, time window, receiving role/initials; timestamped confirmation completes the same task. Confirmation, reopen/undo and invalid-window validation use the task store. |
| Repeat-job templates/checklists | Save any job/checklist; instantiate with fresh identity, dates, ticks and workflow progress; archive/undo. Existing repeating-checklist rules retained. |
| Confidentiality | Reference-only clinic entries; text checker covers supplier, order/invoice references, receiver and template names as well as normal task fields. No patient details required. |
| Data protection | Encrypted store/backups; transactional migrations and shared undo; existing tasks, clinic data and undo retained; restore previews include clinic/template counts. Migration and encrypted recovery tests. |

Platform proof consists of Rust tests, Svelte/TypeScript checks, desktop/mobile
synthetic screen inspection and Android device checks. The release workflow also
builds Windows, Linux and Android and verifies signed release assets before
publication. Real Windows UI/Office-app checks remain manual platform coverage;
they are not missing phase-4 product features.

Phase 5 begins after publication. Its remaining work is smart capture, desktop
hotkey/Hyprland entry, Android share target/tile, focus/time logs, ICS/email import,
and daily planning/review.
