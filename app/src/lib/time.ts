// Dates in the user's local time. The Rust side stores UTC RFC 3339 strings.

const day = 86_400_000

function startOfDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
}

const hm = (d: Date) => d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })

/** "today 15:00", "tomorrow 09:30", "Thu 14:30", "12 Nov 10:00". */
export function when(iso: string | null, now = new Date()): string {
  if (!iso) return ''
  const d = new Date(iso)
  const days = Math.round((startOfDay(d) - startOfDay(now)) / day)
  if (days === 0) return `today ${hm(d)}`
  if (days === 1) return `tomorrow ${hm(d)}`
  if (days === -1) return `yesterday ${hm(d)}`
  if (days > 1 && days < 7) return `${d.toLocaleDateString([], { weekday: 'short' })} ${hm(d)}`
  return `${d.toLocaleDateString([], { day: 'numeric', month: 'short' })} ${hm(d)}`
}

/** "Wednesday 4 November". */
export function longDate(now = new Date()): string {
  return now.toLocaleDateString([], { weekday: 'long', day: 'numeric', month: 'long' })
}

export function greeting(now = new Date()): string {
  const h = now.getHours()
  return h < 12 ? 'Good morning' : h < 18 ? 'Good afternoon' : 'Good evening'
}

/** UTC string to a `datetime-local` input value. */
export function toInput(iso: string | null): string {
  if (!iso) return ''
  const d = new Date(iso)
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`
}

/** A `datetime-local` input value to UTC, or null when empty or invalid. */
export function fromInput(v: string): string | null {
  if (!v) return null
  const d = new Date(v)
  return isNaN(d.getTime()) ? null : d.toISOString()
}

/** Minutes as "45 min" or "1 h 30 min". */
export function minutes(m: number | null): string {
  if (m == null) return ''
  if (m < 60) return `${m} min`
  const h = Math.floor(m / 60)
  return m % 60 ? `${h} h ${m % 60} min` : `${h} h`
}

/** Tomorrow at 09:00 local, as minutes from now. */
export function minutesToTomorrowMorning(now = new Date()): number {
  const t = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 9, 0)
  return Math.max(1, Math.round((t.getTime() - now.getTime()) / 60_000))
}
