import { test } from 'node:test'
import assert from 'node:assert/strict'
import { parseCalendar } from './calendarImport.ts'

const calendar = (body) => `BEGIN:VCALENDAR\r\nVERSION:2.0\r\n${body}\r\nEND:VCALENDAR\r\n`
test('parses events, tasks and folded descriptions', () => {
  const v = parseCalendar(calendar('BEGIN:VEVENT\r\nUID:event-1\r\nSUMMARY:Review stock\r\nDTSTART:20261104T120000Z\r\nDESCRIPTION:Call the\r\n supplier\r\nEND:VEVENT\r\nBEGIN:VTODO\r\nUID:todo-1\r\nSUMMARY:Send report\r\nDUE:20261105T130000Z\r\nEND:VTODO'))
  assert.equal(v.entries.length, 2)
  assert.equal(v.entries[0].when, '2026-11-04T12:00:00.000Z')
  assert.equal(v.entries[0].notes, 'Call thesupplier')
  assert.equal(v.entries[1].kind, 'deadline')
})
test('reports recurring, unknown zones and duplicate IDs rather than silently importing', () => {
  const v = parseCalendar(calendar('BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Repeat\r\nDTSTART:20261104T120000Z\r\nRRULE:FREQ=WEEKLY\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:b\r\nSUMMARY:Unknown zone\r\nDTSTART;TZID=Missing/Zone:20261104T120000\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Repeat override\r\nDTSTART:20261105T120000Z\r\nEND:VEVENT'))
  assert.equal(v.entries.length, 0)
  assert.equal(v.skipped.length, 3)
  assert.match(v.skipped[1], /unknown time zone/)
})
test('all-day entries suggest 5 pm local and invalid files fail', () => {
  const v = parseCalendar(calendar('BEGIN:VEVENT\r\nUID:all-day\r\nSUMMARY:Inventory\r\nDTSTART;VALUE=DATE:20261105\r\nEND:VEVENT'))
  assert.equal(v.entries[0].allDay, true)
  assert.equal(new Date(v.entries[0].when).getHours(), 17)
  assert.throws(() => parseCalendar('not a calendar'))
})


test('rejects normalized invalid dates and cancellation files', () => {
  const v = parseCalendar(calendar('BEGIN:VEVENT\r\nUID:bad\r\nSUMMARY:Bad date\r\nDTSTART:20260230T120000Z\r\nEND:VEVENT'))
  assert.equal(v.entries.length, 0)
  assert.match(v.skipped[0], /out-of-range/)
  assert.throws(() => parseCalendar(calendar('METHOD:CANCEL\r\nBEGIN:VEVENT\r\nUID:x\r\nSUMMARY:Cancelled\r\nDTSTART:20261105T120000Z\r\nEND:VEVENT')), /cancellations/)
})

test('uses a calendar-provided time zone across daylight saving', () => {
  const zone = 'BEGIN:VTIMEZONE\r\nTZID:Europe/London\r\nBEGIN:DAYLIGHT\r\nDTSTART:19700329T010000\r\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\nTZOFFSETFROM:+0000\r\nTZOFFSETTO:+0100\r\nEND:DAYLIGHT\r\nBEGIN:STANDARD\r\nDTSTART:19701025T020000\r\nRRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nTZOFFSETFROM:+0100\r\nTZOFFSETTO:+0000\r\nEND:STANDARD\r\nEND:VTIMEZONE'
  const v = parseCalendar(calendar(`${zone}\r\nBEGIN:VEVENT\r\nUID:s\r\nSUMMARY:Summer\r\nDTSTART;TZID=Europe/London:20260701T090000\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:w\r\nSUMMARY:Winter\r\nDTSTART;TZID=Europe/London:20261201T090000\r\nEND:VEVENT`))
  assert.equal(v.entries[0].when, '2026-07-01T08:00:00.000Z')
  assert.equal(v.entries[1].when, '2026-12-01T09:00:00.000Z')
})
