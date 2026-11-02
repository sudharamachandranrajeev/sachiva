import ICAL from 'ical.js'

export interface CalendarDraft {
  uid: string
  title: string
  notes: string
  when: string
  kind: 'appointment' | 'deadline'
  allDay: boolean
}
export interface CalendarPreview { entries: CalendarDraft[]; skipped: string[] }

/** Parse locally. Unsupported recurring or ambiguous entries are explicitly skipped. */
export function parseCalendar(text: string): CalendarPreview {
  if (new TextEncoder().encode(text).length > 2_000_000) throw new Error('Choose a calendar file under 2 MB.')
  let root: ICAL.Component
  try { root = new ICAL.Component(ICAL.parse(text)) }
  catch { throw new Error('This is not a readable calendar file.') }
  if (root.name !== 'vcalendar') throw new Error('This is not an iCalendar file.')
  if (String(root.getFirstPropertyValue('method') ?? '').toUpperCase() === 'CANCEL') throw new Error('This file contains calendar cancellations, not new jobs.')
  const components = root.getAllSubcomponents().filter((c) => c.name === 'vevent' || c.name === 'vtodo')
  if (components.length > 100) throw new Error('Choose a calendar containing at most 100 events and tasks.')
  const entries: CalendarDraft[] = []
  const skipped: string[] = []
  const seen = new Set<string>()
  for (const component of components) {
    const rawTitle = component.getFirstPropertyValue('summary')
    const title = typeof rawTitle === 'string' ? rawTitle.trim() : ''
    const label = title || 'Untitled entry'
    const skip = (reason: string) => skipped.push(`${label}: ${reason}`)
    const uidValue = component.getFirstPropertyValue('uid')
    const uid = typeof uidValue === 'string' ? uidValue.trim() : ''
    if (!uid || uid.length > 1024 || /[\x00-\x1f\x7f]/.test(uid)) { skip('missing or invalid calendar ID'); continue }
    if (seen.has(uid)) { skip('duplicate calendar ID in this file'); continue }
    seen.add(uid)
    if (!title || Array.from(title).length > 300) { skip('title is missing or too long'); continue }
    const status = String(component.getFirstPropertyValue('status') ?? '').toUpperCase()
    if (status === 'CANCELLED' || status === 'COMPLETED') { skip('cancelled or completed'); continue }
    if (['rrule', 'rdate', 'exdate', 'recurrence-id'].some((p) => component.getFirstProperty(p))) {
      skip('repeating or modified series is not supported yet'); continue
    }
    const description = component.getFirstPropertyValue('description')
    const notes = typeof description === 'string' ? description.trim() : ''
    if (Array.from(notes).length > 20_000) { skip('description is too long'); continue }
    const prop = component.getFirstProperty(component.name === 'vevent' ? 'dtstart' : 'due')
    if (!prop) { skip('no start or due date'); continue }
    try {
      const raw = prop.toJSON()[3]
      const time = prop.getFirstValue()
      if (!(time instanceof ICAL.Time)) { skip('invalid date'); continue }
      if (typeof raw !== 'string' || time.toString() !== raw) { skip('invalid or out-of-range date'); continue }
      const tzid = prop.getParameter('tzid')
      if (tzid && time.zone === ICAL.Timezone.localTimezone) { skip(`unknown time zone ${String(tzid)}`); continue }
      const date = time.toJSDate()
      if (time.isDate) date.setHours(17, 0, 0, 0)
      else if (time.zone === ICAL.Timezone.localTimezone && (date.getFullYear() !== time.year || date.getMonth() + 1 !== time.month || date.getDate() !== time.day || date.getHours() !== time.hour || date.getMinutes() !== time.minute)) { skip('local time does not exist'); continue }
      if (Number.isNaN(date.getTime())) { skip('invalid date'); continue }
      entries.push({ uid, title, notes, when: date.toISOString(), kind: component.name === 'vevent' ? 'appointment' : 'deadline', allDay: time.isDate })
    } catch { skip('invalid date or time zone') }
  }
  return { entries, skipped }
}
