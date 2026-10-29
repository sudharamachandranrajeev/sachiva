// Update state shared by the start-up banner and Settings → Updates.
// Sachiva checks once at start (unless turned off) and never installs
// without the user pressing the button.

import { api, message, onUpdateProgress, type InstallKind, type UpdateInfo, type UpdateStatus } from './api'
import { app } from './state.svelte'

export const kindName: Record<InstallKind, string> = {
  installer: 'installed with the Windows installer',
  appImage: 'the AppImage',
  pacman: 'the Arch Linux package',
  portable: 'the portable version for Windows',
  android: 'the Android app',
  other: 'a copy built from source or installed another way',
}

class Updates {
  status = $state<UpdateStatus | null>(null)
  found = $state<UpdateInfo | null>(null)
  checked = $state(false)
  busy = $state(false)
  /** 0 to 1 while downloading, null otherwise. */
  progress = $state<number | null>(null)
  /** The banner was put away for this run. */
  dismissed = $state(false)
  #listening = false

  async load() {
    this.status = await api.updateStatus()
  }

  /** At start: check quietly unless updates are off. Offline is not an error here. */
  async startup() {
    try {
      await this.load()
      if (this.status?.mode === 'ask') await this.check(true)
    } catch {
      // Status could not be read: Settings shows the problem.
    }
  }

  async check(quiet = false) {
    try {
      this.found = await api.checkUpdate()
      this.checked = true
      if (!quiet && !this.found) app.say('Sachiva is up to date.')
    } catch (e) {
      if (!quiet) app.say(message(e), { error: true })
    }
  }

  async #listen() {
    if (this.#listening) return
    this.#listening = true
    await onUpdateProgress((done, total) => (this.progress = total ? done / total : null))
  }

  /** Back up, download, check the signature, install and restart. */
  async install() {
    if (!this.found) return
    this.busy = true
    await this.#listen()
    try {
      await api.installUpdate(this.found.version)
      if (this.status?.kind === 'android') app.say('Android is asking you to confirm the update.')
    } catch (e) {
      app.say(message(e), { error: true })
    } finally {
      this.busy = false
      this.progress = null
    }
  }

  async rollback(restore: boolean) {
    this.busy = true
    await this.#listen()
    try {
      await api.rollback(restore)
    } catch (e) {
      app.say(message(e), { error: true })
    } finally {
      this.busy = false
      this.progress = null
    }
  }
}

export const updates = new Updates()
