# Changelog

## [0.2.0]

Targets, tallies, waiting lists and reports (alpha). Updating keeps your jobs; Sachiva backs up first.

Targets, waiting lists and reports:

- Type of job: general, with a deadline, target, tally, chase, recurring checklist, appointment or call.
- Targets per day, week or month, or a total by a date. Progress bar, even-pace marker, "Behind" flag,
  "met in N of the last M", and a working-hours reminder when a repeating target falls behind.
- Tallies count up from 0 with no goal. +1, +N and Undo last on the dashboard; corrections are new
  entries, never edits.
- Waiting lists with site, clinic, provider (initials or role), wait bands and a target wait. Enter
  figures daily, weekly or monthly: total, added, removed, booked, each band and the longest wait.
  Blank means not entered, never 0. Lists are archived, never deleted.
- Reports: personal performance and waiting lists, as control charts in the "Making Data Count"
  style (process limits, special-cause colours, assurance against a target). Preview on screen; save
  as a spreadsheet (.xlsx), document (.docx), slides (.pptx) or PDF (tagged PDF/UA-1, alt text on every chart, no logos).
- The period still running is left out of performance charts and shown as "so far" in the table.

## [0.1.0]

The first release (alpha). Installers for Windows, Linux and Android.

Releases and updates:

- Windows installer (no admin rights needed) and a portable zip. Linux AppImage and Arch package.
  Android APK. Every release lists SHA-256 checksums.
- Checks for a new version at start and asks before installing. Backs up your data first. Every
  download is checked against Sachiva's signature (desktop) or checksum and signing key (Android).
- Go back to the version before the last update (desktop), restoring the matching backup if needed.
- The portable version tells you when a new version is out, since it cannot update itself.
- Backups screen: daily, before updates, made by you, before a restore. Restore any of them after
  seeing what it holds; your current data is backed up first.
- Export a copy locked with a passphrase, and restore from such a file or from another device's
  backup with its recovery key.
- Report a problem / Suggest a feature: opens a GitHub form with version, system, look settings and
  the last three error messages filled in. No job text.
- First-run steps: reminders, updates, look. All skippable.

The foundation:

- Jobs with deadline, appointment time, work needed, importance, site, tags, checklist and notes.
- "Waiting on someone" with a chase date.
- Repeats: every few days, weekly on chosen days, monthly on a day (kept on the 31st where it exists),
  or some days after it was last done. Safe across clock changes.
- Priority score from 0 to 99 with the reason in plain words.
- Dashboard: Do this now, Overdue, Today, Waiting on others, Later, and five counts that explain
  themselves. Refreshes every 30 seconds and on every change.
- Tasks: search and filter by lane, site and tag; Done list with Put back; Trash kept for 30 days.
- Quick capture (N), autosaving editor, undo (Ctrl+Z).
- Confidentiality checker: health numbers (with the check digit), phone numbers, birth dates,
  postcodes, e-mail addresses, titled names and street addresses. It warns and never blocks.
- Reminders: heads-up, due, repeats in working hours, urgent repeats, quiet after an edit, snooze.
  Desktop: tray, starts with the computer. Android: exact alarms planned three days ahead.
- Encrypted database (SQLCipher) with a key in the system password store or Android Keystore,
  recovery key on first run, unlock with the recovery key, daily encrypted backups (14 kept).
- Light and dark colours, calm (reduced) movement.
