# Android device checks

Use synthetic jobs and counts only. Install over the existing app with the same
signing key; do not uninstall or clear storage to test an update.

## Device run, 2026-10-31

Tested on a physical Android 17 phone with three-button navigation:

- Reproduced the published 0.2.0 startup stall with notifications already allowed;
  revoking permission and answering the prompt unblocked startup.
- With the fix, two consecutive cold starts opened the saved tally in about
  2.6 seconds each. Installing over the published app preserved the tally/count.
- The optimized, signed release build was then installed over the diagnostic
  build. Repeated launch, system-bar spacing and all three completed synthetic
  jobs were verified again; the installed app is not debuggable.
- Dashboard and editor clear system bars in portrait; landscape layout clears
  the side navigation bar. Editor actions stay visible above the keyboard.
- All four report formats saved through the system picker. ZIP/XML validation
  passed for the Office formats; PDF parsing reported four pages and tagging.
  Cancelling a save returned false.
- With the app backgrounded, Android delivered the weekly pace notification at
  its scheduled time and the due notification 20 seconds later. Completing the
  jobs removed their future alarms; original reminder settings were restored.
- Workspace checks: 69 tests passed, one manual export dump ignored; desktop
  clippy, UI checks/build, formatting and leak scans passed.

Gesture navigation, older Android versions and opening these exports in
Microsoft Office remain separate checks.

## Clinic preview 0.3.0, 2026-10-31

- Created a manual encrypted backup before upgrading the phone from schema 2.
- Installed the optimized APK signed with the existing key; the previous three
  completed synthetic tasks remained visible after migration.
- Created a synthetic clinic list, waiting reference and cancelled slot through
  the phone forms. The matching reference appeared with its target-date reason.
- Logged a no-answer call, confirmed a booking, and verified the filled outcome
  and dashboard refill rate. Undo restored the open slot and waiting reference.
- Marked the slot lost and verified the dashboard; undo restored it again.
- An open slot scheduled three Android alarms (heads-up, due and schedule-renewal
  reminder). After undoing the synthetic call, slot and reference, no Sachiva
  alarms remained. The original reminder settings were unchanged.
- Desktop and mobile screenshots were checked using synthetic harness data.
- Core tests cover fit/ranking, local time across a clock change, atomic filling,
  duplicate bookings, stale edits, undo, migration of old lists/figures/history,
  stable reference/list identity and encrypted clinic backup recovery.
- Final checks: 77 workspace tests passed, one manual export dump ignored;
  clippy, UI checking/build, formatting and the complete public-file leak scan passed.

## Startup and data preservation

1. Allow notifications and finish setup. Create a test tally and add a count.
2. Force-stop and launch the app twice with notification permission still granted.
   Both launches must leave "Opening…" and show the saved tally and count.
3. Install the candidate APK over that app. Repeat step 2.
4. On a test installation, revoke notification permission and launch again. Answer
   the permission prompt, then verify that startup completes. Restore permission.

The permission check before requesting notifications is intentional: version 2.8.0
of the notification plugin does not resolve Android's request when permission is
already granted. An unconditional request during setup blocks subsequent launches.

## System bars and keyboard

1. Select Android's three-button navigation. Open the dashboard and task editor.
2. Verify the title and Close button sit below the status bar, and the editor's
   bottom actions and app navigation sit above the system navigation bar. Tap Close.
3. Focus an editor input. Verify the editor resizes above the keyboard and its
   bottom actions remain visible. Dismiss the keyboard and check layout again.
4. Repeat after rotation and with gesture navigation when available.

The device plugin applies native window insets to the content container, consuming
them before the WebView. This covers the whole app, including full-screen HTML
dialogs, and avoids relying on WebView CSS safe-area values that can be zero.

## Reports

1. Make a report from the synthetic tally. Save each of `.xlsx`, `.docx`, `.pptx`
   and `.pdf` through Android's file picker.
2. Verify each command reports success and each saved file has nonzero size.
3. Verify ZIP integrity and XML parts for the three Office formats; parse the PDF
   and check that it is tagged. This does not replace testing in Microsoft Office.
4. Cancel a save dialog. The command must return false, release the busy state,
   and leave an existing report unchanged.

## Background reminders

1. Allow notifications, exact alarms and unrestricted battery use.
2. Record the original reminder settings. Temporarily disable the quiet-after-edit
   interval and include the current day and time in working hours.
3. Create a test deadline shortly ahead. Create a weekly target behind pace after
   two thirds of its week, setting working-hours start shortly ahead so a pace
   reminder is scheduled then. Do not change the phone's clock.
4. Background the app. Verify both notifications arrive with the expected title.
5. Complete or trash the test reminder jobs and restore the original settings.
   Verify their pending alarms are cancelled.
