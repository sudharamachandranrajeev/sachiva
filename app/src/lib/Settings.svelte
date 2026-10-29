<script lang="ts">
  import { onMount } from 'svelte'
  import { getVersion } from '@tauri-apps/api/app'
  import { api, message, type AlarmAccess, type Page, type ReminderSettings } from './api'
  import { app, recentErrors } from './state.svelte'
  import { prefs, type Motion, type Theme } from './prefs.svelte'
  import BackupsPanel from './BackupsPanel.svelte'
  import UpdatesPanel from './UpdatesPanel.svelte'

  let { alarms, onAlarms }: { alarms: AlarmAccess | null; onAlarms: () => void } = $props()

  let s = $state<ReminderSettings | null>(null)
  let saved = $state<ReminderSettings | null>(null)
  let autostart = $state<boolean | null>(null)
  let key = $state('')
  let version = $state('')
  let error = $state('')

  const hhmm = (t: string) => t.slice(0, 5)
  const dirty = $derived(JSON.stringify(s) !== JSON.stringify(saved))
  const where: Record<string, string> = {
    keyring: "your computer's password store (the system keyring)",
    file: 'a private file next to your data, because no system keyring was found',
    androidKeystore: "the phone's secure key store (Android Keystore)",
  }

  onMount(async () => {
    try {
      s = await api.settings()
      saved = $state.snapshot(s)
      version = await getVersion()
      if (app.status?.platform !== 'android') autostart = await api.autostart()
    } catch (e) {
      error = message(e)
    }
  })

  async function saveReminders() {
    if (!s) return
    // jiff reads "08:00:00"; the time input gives "08:00".
    const out = { ...s, workStart: hhmm(s.workStart) + ':00', workEnd: hhmm(s.workEnd) + ':00' }
    try {
      await api.setSettings(out)
      saved = $state.snapshot(s)
      app.say('Reminder settings saved.')
    } catch (e) {
      app.say(message(e), { error: true })
    }
  }

  async function setAutostart(on: boolean) {
    try {
      await api.setAutostart(on)
      autostart = await api.autostart()
    } catch (e) {
      app.say(message(e), { error: true })
    }
  }

  // Opens GitHub in the browser with the details filled in. Nothing is sent
  // until you press Submit there, and no job text is included.
  async function report(page: Page) {
    try {
      await api.openPage(page, { theme: prefs.theme, motion: prefs.motion, errors: recentErrors().join('\n') })
    } catch (e) {
      app.say(message(e), { error: true })
    }
  }

  async function showKey() {
    try {
      key = await api.recoveryKey()
    } catch (e) {
      app.say(message(e), { error: true })
    }
  }

  const num = (v: unknown, min: number, max: number) => Math.min(max, Math.max(min, Math.round(Number(v) || min)))
</script>

<h1>Settings</h1>
{#if error}<p class="error">{error}</p>{/if}

<section class="card">
  <h2>Reminders</h2>
  <p class="muted">
    Sachiva reminds you before a deadline, when it is due, and again until the job is done. These settings decide how
    often.
  </p>
  {#if s}
    <div class="grid">
      <label class="field">
        Repeat overdue jobs every (minutes)
        <input type="number" min="1" max="1440" bind:value={() => s!.repeatEveryMin, (v) => (s!.repeatEveryMin = num(v, 1, 1440))} />
        <span class="hint">Also used for jobs waiting on someone once it is time to chase.</span>
      </label>
      <label class="field">
        Repeat urgent jobs every (minutes)
        <input type="number" min="1" max="1440" bind:value={() => s!.urgentEveryMin, (v) => (s!.urgentEveryMin = num(v, 1, 1440))} />
        <span class="hint">Urgent jobs repeat from the heads-up until you act on them.</span>
      </label>
      <label class="field">
        A job is urgent from score
        <input type="number" min="1" max="99" bind:value={() => s!.urgentScore, (v) => (s!.urgentScore = num(v, 1, 99))} />
        <span class="hint">The score runs from 0 to 99. The default, 85, is roughly "important and due within hours".</span>
      </label>
      <label class="field">
        Quiet after an edit (minutes)
        <input
          type="number"
          min="0"
          max="1440"
          bind:value={() => s!.quietAfterEditMin, (v) => (s!.quietAfterEditMin = num(v, 0, 1440))}
        />
        <span class="hint">No reminder for a job you have just added or changed.</span>
      </label>
      <label class="field">
        Working day starts
        <input type="time" bind:value={() => hhmm(s!.workStart), (v) => (s!.workStart = v)} />
      </label>
      <label class="field">
        Working day ends
        <input type="time" bind:value={() => hhmm(s!.workEnd), (v) => (s!.workEnd = v)} />
      </label>
    </div>
    <label class="check-row">
      <input type="checkbox" bind:checked={s.weekends} />
      Repeat reminders at weekends too
    </label>
    <p class="hint">
      Repeats only come in working hours. The first heads-up and the "due now" reminder always come, at any hour, because
      you set those times yourself.
    </p>
    <button class="btn p" disabled={!dirty} onclick={saveReminders}>Save reminder settings</button>
  {/if}
</section>

{#if alarms}
  <section class="card">
    <h2>Alarms on this phone</h2>
    <p class="muted">
      Android can hold reminders back to save battery. For reminders on time, Sachiva needs both of these. Reminders are
      planned three days ahead; open Sachiva at least every few days to keep them coming.
    </p>
    <div class="status">
      <span class="pill {alarms.exact ? '' : 'bad'}">{alarms.exact ? 'Allowed' : 'Not allowed'}</span>
      <span>Exact alarms: reminders at the minute you set.</span>
      {#if !alarms.exact}
        <button class="btn" onclick={() => api.openAlarmSettings().finally(onAlarms)}>Allow</button>
      {/if}
    </div>
    <div class="status">
      <span class="pill {alarms.batteryUnrestricted ? '' : 'warn'}"
        >{alarms.batteryUnrestricted ? 'Unrestricted' : 'Restricted'}</span
      >
      <span>Battery use: some phones delay or drop reminders when this is restricted.</span>
      {#if !alarms.batteryUnrestricted}
        <button class="btn" onclick={() => api.openBatterySettings().finally(onAlarms)}>Change</button>
      {/if}
    </div>
  </section>
{/if}

<section class="card">
  <h2>Look</h2>
  <div class="grid">
    <label class="field">
      Colours
      <select bind:value={prefs.theme} onchange={() => prefs.apply()}>
        {#each [['system', 'Follow the system'], ['light', 'Light'], ['dark', 'Dark']] as [v, name] (v)}
          <option value={v as Theme}>{name}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      Movement
      <select bind:value={prefs.motion} onchange={() => prefs.apply()}>
        {#each [['system', 'Follow the system'], ['reduced', 'Calm: as little as possible'], ['full', 'Full']] as [v, name] (v)}
          <option value={v as Motion}>{name}</option>
        {/each}
      </select>
    </label>
  </div>
  <p class="hint">These are kept on this device only.</p>
</section>

<section class="card">
  <h2>Your data</h2>
  <p class="muted">
    Everything stays on this device, in an encrypted database. Nothing is sent anywhere. The key that opens it is kept in
    {app.status?.keyStorage ? where[app.status.keyStorage] : 'a safe place on this device'}.
  </p>

  <h3>Recovery key</h3>
  <p class="hint">
    If this device's key store breaks, the recovery key is the only way to open your data and your backups. Keep it on
    paper, away from the device.
  </p>
  {#if key}
    <code class="key">{key}</code>
    <button class="btn" onclick={() => (key = '')}>Hide</button>
  {:else}
    <button class="btn" onclick={showKey}>Show recovery key</button>
  {/if}

  <BackupsPanel />

  <h3>Deleted jobs</h3>
  <p class="hint">Deleted jobs stay in the trash for 30 days, so you can restore them from Tasks, then go for good.</p>
</section>

<UpdatesPanel />

<section class="card">
  <h2>Help and feedback</h2>
  <p class="muted">
    These open a form on GitHub in your browser, with Sachiva's version, your system, your look settings and the last
    three error messages filled in. No job text is included. Nothing is sent until you press Submit there, and you can
    edit everything first. You need a free GitHub account.
  </p>
  <div class="row">
    <button class="btn" onclick={() => report('bug')}>Report a problem</button>
    <button class="btn" onclick={() => report('idea')}>Suggest a feature</button>
  </div>
</section>

<section class="card">
  <h2>About</h2>
  <p class="muted">Sachiva {version}. Free and open source under the MIT licence.</p>
  {#if autostart !== null}
    <label class="check-row">
      <input type="checkbox" checked={autostart} onchange={(e) => setAutostart(e.currentTarget.checked)} />
      Start Sachiva with the computer
    </label>
    <p class="hint">
      Sachiva keeps running in the tray, so reminders arrive with the window closed. If it does not start with the
      computer, reminders only come after you open it. Quit from the tray icon to stop it.
    </p>
  {/if}
</section>

<style>
  section {
    padding: 18px 20px;
    margin-top: 16px;
    max-width: 860px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    align-items: flex-start;
  }
  section h3 {
    margin-top: 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
    width: 100%;
  }
  .hint {
    font-size: 12px;
    font-weight: 400;
    color: var(--muted);
  }
  .check-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .status {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
  }
  .key {
    font-size: 15px;
    letter-spacing: 0.04em;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--panel2);
    border: 1px solid var(--line);
    overflow-wrap: anywhere;
    user-select: all;
  }
  code {
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .error {
    color: var(--urgent);
  }
  @media (max-width: 760px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
