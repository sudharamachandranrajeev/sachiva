import * as chrono from 'chrono-node'
import type { Importance } from './api'

export interface NoteDraft {
  title: string
  due: string | null
  importance: Importance
  dateHadTime: boolean
}

/** Extract suggestions only. The original note stays intact and the user reviews the result. */
export function draftFromNote(note: string, now = new Date()): NoteDraft {
  const lines = note.split(/\r?\n/).map((line) => line.trim()).filter(Boolean)
  const labelledTitle = lines.find((line) => /^(?:title|subject|task)\s*:/i.test(line))
  const first = labelledTitle?.replace(/^(?:title|subject|task)\s*:\s*/i, '') || lines[0] || ''
  const title = Array.from(first).slice(0, 300).join('').trim()
  const priority = note.match(/(?:^|\n)\s*(?:priority|importance)\s*:\s*(high|urgent|normal|medium|low)\b/i)?.[1]?.toLowerCase()
  const importance: Importance = priority === 'high' || priority === 'urgent' ? 'high'
    : priority === 'low' ? 'low'
    : priority ? 'normal'
    : /(?:^|\s)!high\b|\burgent\b/i.test(note) ? 'high' : 'normal'
  const deadlineLine = lines.find((line) => /^(?:due|deadline|due date)\s*:/i.test(line))
  const source = deadlineLine ? deadlineLine.replace(/^[^:]+:\s*/, '') : note
  const result = chrono.en.GB.parse(source, now, { forwardDate: true })[0]
  let due: string | null = null
  let dateHadTime = false
  if (result) {
    dateHadTime = result.start.isCertain('hour')
    const date = result.start.date()
    if (!dateHadTime) date.setHours(17, 0, 0, 0)
    if (!Number.isNaN(date.getTime())) due = date.toISOString()
  }
  return { title, due, importance, dateHadTime }
}

/** Parse the explicit shorthand used by quick capture; always review before saving. */
export function draftFromQuick(text: string, now = new Date()): NoteDraft & { site: string } {
  const parsed = draftFromNote(text, now)
  const date = chrono.en.GB.parse(text, now, { forwardDate: true })[0]
  const site = text.match(/(?:^|\s)@([\p{L}\p{N}_-]+)/u)?.[1] ?? ''
  const priority = text.match(/(?:^|\s)!(high|normal|low)\b/i)?.[1]?.toLowerCase() as Importance | undefined
  let title = text
  if (date) title = title.slice(0, date.index) + ' ' + title.slice(date.index + date.text.length)
  title = title.replace(/(?:^|\s)!(?:high|normal|low)\b/gi, ' ')
    .replace(/(?:^|\s)@[\p{L}\p{N}_-]+/gu, ' ')
    .replace(/\s+/g, ' ').trim()
  return { ...parsed, title: Array.from(title).slice(0, 300).join(''), site, importance: priority ?? parsed.importance }
}
