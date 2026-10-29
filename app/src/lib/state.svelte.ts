// What the whole UI shares: the dashboard, the open editor, the message bar.

import { api, message, type AppStatus, type Dashboard, type Task } from './api'

export type View = 'dashboard' | 'tasks' | 'settings'

interface Toast {
  text: string
  undo: boolean
  error: boolean
}

class AppState {
  status = $state<AppStatus | null>(null)
  dash = $state<Dashboard | null>(null)
  view = $state<View>('dashboard')
  editing = $state<{ task: Task; isNew: boolean } | null>(null)
  capturing = $state(false)
  toast = $state<Toast | null>(null)
  /** Bumped on every change so lists that fetch their own data reload. */
  version = $state(0)
  #timer: ReturnType<typeof setTimeout> | undefined

  async loadStatus() {
    this.status = await api.status()
  }

  async refresh() {
    if (!this.status?.unlocked) return
    try {
      this.dash = await api.dashboard()
      this.version++
    } catch (e) {
      this.say(message(e), { error: true })
    }
  }

  say(text: string, { undo = false, error = false } = {}) {
    clearTimeout(this.#timer)
    this.toast = { text, undo, error }
    this.#timer = setTimeout(() => (this.toast = null), undo ? 8000 : 4000)
  }

  /** Run a change; on success show `done` with an Undo button. */
  async act(f: () => Promise<unknown>, done?: string) {
    try {
      await f()
      if (done) this.say(done, { undo: true })
    } catch (e) {
      this.say(message(e), { error: true })
    }
  }

  async undo() {
    try {
      const label = await api.undo()
      this.say(label ? `Undone: ${label}.` : 'Nothing to undo.')
    } catch (e) {
      this.say(message(e), { error: true })
    }
  }

  /** Open the editor on a copy of a saved job. */
  edit(task: Task) {
    this.editing = { task: $state.snapshot(task), isNew: false }
  }

  /** Open the editor on a fresh job; it is saved once it has a title. */
  async startNew(title = '') {
    this.editing = { task: await api.newTask(title), isNew: true }
  }

  /** All site names in use, for suggestions and filters. */
  get sites(): string[] {
    const all = [this.dash?.now, ...(this.dash?.items ?? [])].map((i) => i?.task.site.trim() ?? '')
    return [...new Set(all.filter(Boolean))].sort()
  }

  get tags(): string[] {
    const all = [this.dash?.now, ...(this.dash?.items ?? [])].flatMap((i) => i?.task.tags ?? [])
    return [...new Set(all)].sort()
  }
}

export const app = new AppState()
