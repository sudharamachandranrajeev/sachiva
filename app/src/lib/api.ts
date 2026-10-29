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
  /** Something to tell the user once, such as a failed restore at start. */
  notice: string | null
  setupDone: boolean
}

export interface AlarmAccess {
  exact: boolean
  batteryUnrestricted: boolean
}

export type BackupKind = 'daily' | 'manual' | 'preUpdate' | 'beforeRestore'

export interface Backup {
  name: string
  kind: BackupKind
  bytes: number
}

export interface Backups {
  folder: string
  files: Backup[]
}

export interface BackupSummary {
  schema: number
  openJobs: number
  doneJobs: number
  trashed: number
  lastChange: string | null
}

export type InstallKind = 'installer' | 'appImage' | 'pacman' | 'portable' | 'android' | 'other'
export type UpdateMode = 'ask' | 'off'

export interface UpdateStatus {
  kind: InstallKind
  canInstall: boolean
  current: string
  previous: string | null
  mode: UpdateMode
}

export interface UpdateInfo {
  version: string
  notes: string | null
  date: string | null
  canInstall: boolean
}

export interface RollbackPlan {
  version: string
  backup: string | null
  needsRestore: boolean
}

export type Page = 'releases' | 'bug' | 'idea'


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
  setupDone: () => invoke<void>('setup_done'),
  autostart: () => invoke<boolean>('autostart'),
  setAutostart: (on: boolean) => invoke<void>('set_autostart', { on }),
  backups: () => invoke<Backups>('backups'),
  backupNow: () => invoke<string>('backup_now'),
  /** False when the user cancelled the save dialog. */
  exportBackup: (passphrase: string) => invoke<boolean>('export_backup', { passphrase }),
  /** The picked file's name, or null when cancelled. */
  pickBackup: () => invoke<string | null>('pick_backup'),
  /** `name`: a backup in the folder; without it, the picked file with `passphrase`. */
  inspectBackup: (name: string | null, passphrase: string | null) =>
    invoke<BackupSummary>('inspect_backup', { name, passphrase }),
  restoreBackup: (name: string | null, passphrase: string | null) =>
    invoke<string>('restore_backup', { name, passphrase }),
  updateStatus: () => invoke<UpdateStatus>('update_status'),
  setUpdateMode: (mode: UpdateMode) => invoke<void>('set_update_mode', { mode }),
  checkUpdate: () => invoke<UpdateInfo | null>('check_update'),
  installUpdate: (version: string) => invoke<void>('install_update', { version }),
  rollbackPlan: () => invoke<RollbackPlan | null>('rollback_plan'),
  rollback: (restore: boolean) => invoke<void>('rollback', { restore }),
  openPage: (page: Page, fields: Record<string, string> = {}) => invoke<void>('open_page', { page, fields }),
  alarmAccess: () => invoke<AlarmAccess>('alarm_access'),
  openAlarmSettings: () => invoke<void>('open_alarm_settings'),
  openBatterySettings: () => invoke<void>('open_battery_settings'),
}

/** Download progress of an update: bytes so far and the total, if known. */
export function onUpdateProgress(f: (done: number, total: number | null) => void): Promise<UnlistenFn> {
  return listen<[number, number | null]>('update-progress', (e) => f(e.payload[0], e.payload[1]))
}

/** Calls `f` whenever the data changes on the Rust side. */
export function onChanged(f: () => void): Promise<UnlistenFn> {
  return listen('changed', f)
}

/** Commands reject with a plain-language string; anything else becomes one. */
export function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : 'Something went wrong.'
}
