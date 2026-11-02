import { test } from 'node:test'
import assert from 'node:assert/strict'
import { draftFromNote } from './noteCapture.ts'

const now = new Date(2026, 10, 2, 10, 0)
test('extracts a subject, deadline and explicit priority', () => {
  const d = draftFromNote('Subject: Order stock\nDeadline: Friday 2pm\nPriority: high\nCall supplier', now)
  assert.equal(d.title, 'Order stock')
  assert.equal(d.importance, 'high')
  const date = new Date(d.due)
  assert.equal(date.getDay(), 5)
  assert.equal(date.getHours(), 14)
  assert.equal(d.dateHadTime, true)
})
test('suggests an explicit time for date-only deadlines', () => {
  const d = draftFromNote('Task: Prepare report\nDue: tomorrow\nPriority: low', now)
  assert.equal(new Date(d.due).getHours(), 17)
  assert.equal(d.dateHadTime, false)
  assert.equal(d.importance, 'low')
})
test('handles notes without dates and respects explicit normal priority', () => {
  const d = draftFromNote('Subject: Review urgent request\nPriority: normal', now)
  assert.equal(d.due, null)
  assert.equal(d.importance, 'normal')
})

test('quick capture extracts shorthand without losing the task title', async () => {
  const { draftFromQuick } = await import('./noteCapture.ts')
  const d = draftFromQuick('PO chase fri 2pm !high @site2', now)
  assert.equal(d.title, 'PO chase')
  assert.equal(d.site, 'site2')
  assert.equal(d.importance, 'high')
  assert.equal(new Date(d.due).getHours(), 14)
})
test('quick capture does not treat email addresses as sites', async () => {
  const { draftFromQuick } = await import('./noteCapture.ts')
  const d = draftFromQuick('Reply to person@example.com !low', now)
  assert.equal(d.site, '')
  assert.equal(d.title, 'Reply to person@example.com')
  assert.equal(d.importance, 'low')
})
