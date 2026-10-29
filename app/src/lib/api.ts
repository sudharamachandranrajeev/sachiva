// Types and calls for the Rust side. Field names match core's serde output.

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export type Importance = 'low' | 'normal' | 'high'
export type Status =
  | { state: 'open' }
  | { state: 'waiting'; on: string; chase: string | null }
  | { state: 'done'; at: string }
export type Weekday = 'mon' | 'tue' | 'wed' | 'thu' | 'fri' | 'sat' | 'sun'
export type Repeat =
  | { kind: 'daily'; every: number }
  | { kind: 'weekly'; every: number; days: Weekday[] }
  | { kind: 'monthly'; every: number; day: number }
  | { kind: 'afterDone'; days: number }

export interface CheckItem {
  text: string
  done: boolean
}

export interface Task {
  id: string
  title: string
  notes: string
  due: string | null
  appointment: string | null
  effortMin: number | null
  importance: Importance
  status: Status
  pinned: boolean
  site: string
  tags: string[]
  checklist: CheckItem[]
  repeat: Repeat | null
  snoozedUntil: string | null
  createdAt: string
  updatedAt: string
}

export type Lane = 'overdue' | 'today' | 'waiting' | 'later'

export interface Item {
  task: Task
  score: number
  why: string
  overdue: boolean
  lane: Lane
}

export interface Figure {
  value: number
  label: string
  explain: string
  tone: '' | 'bad' | 'warn'
}

export interface Dashboard {
  now: Item | null
  items: Item[]
  figures: Figure[]
}

export interface Finding {
  kind: string
  start: number
  end: number
  text: string
  message: string
}

export interface Trashed {
  task: Task
  deletedAt: string
}

export interface ReminderSettings {
  quietAfterEditMin: number
  repeatEveryMin: number
  urgentEveryMin: number
  urgentScore: number
  workStart: string
  workEnd: string
  weekends: boolean
}

export type KeyStorage = 'keyring' | 'file' | 'androidKeystore'

export interface AppStatus {
  unlocked: boolean
  problem: string | null
  recoveryPending: boolean
  keyStorage: KeyStorage | null
  platform: string
}

export interface AlarmAccess {
  exact: boolean
  batteryUnrestricted: boolean
}

export interface Backups {
  folder: string
  files: string[]
}

export const api = {
  status: () => invoke<AppStatus>('status'),
  recoveryKey: () => invoke<string>('recovery_key'),
  recoverySaved: () => invoke<void>('recovery_saved'),
  unlockWith: (recovery: string) => invoke<void>('unlock_with', { recovery }),
  dashboard: () => invoke<Dashboard>('get_dashboard'),
  doneTasks: () => invoke<Task[]>('done_tasks'),
  newTask: (title: string) => invoke<Task>('new_task', { title }),
  saveTask: (task: Task) => invoke<Task>('save_task', { task }),
  complete: (id: string) => invoke<void>('complete', { id }),
  reopen: (id: string) => invoke<void>('reopen', { id }),
  deleteTask: (id: string) => invoke<void>('delete_task', { id }),
  restoreTask: (id: string) => invoke<void>('restore_task', { id }),
  trash: () => invoke<Trashed[]>('trash'),
  undo: () => invoke<string | null>('undo'),
  snooze: (id: string, minutes: number) => invoke<void>('snooze', { id, minutes }),
  check: (text: string) => invoke<Finding[]>('check_text', { text }),
  settings: () => invoke<ReminderSettings>('get_settings'),
  setSettings: (settings: ReminderSettings) => invoke<void>('set_settings', { settings }),
  backups: () => invoke<Backups>('backups'),
  backupNow: () => invoke<string>('backup_now'),
  alarmAccess: () => invoke<AlarmAccess>('alarm_access'),
  openAlarmSettings: () => invoke<void>('open_alarm_settings'),
  openBatterySettings: () => invoke<void>('open_battery_settings'),
}

/** Calls `f` whenever the data changes on the Rust side. */
export function onChanged(f: () => void): Promise<UnlistenFn> {
  return listen('changed', f)
}

/** Commands reject with a plain-language string; anything else becomes one. */
export function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : 'Something went wrong.'
}
