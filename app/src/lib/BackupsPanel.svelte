<script lang="ts">
  import { onMount } from 'svelte'
  import { api, message, type BackupKind, type BackupSummary, type Backups } from './api'
  import { app } from './state.svelte'

  let backups = $state<Backups | null>(null)
  let all = $state(false)
  /** The backup being looked at before a restore: a name, or the picked file. */
  let chosen = $state<{ name: string | null; label: string; summary: BackupSummary | null } | null>(null)
  let pass = $state('')
  let exportPass = $state('')
  let exportPass2 = $state('')
  let exporting = $state(false)
  let busy = $state(false)
  let error = $state('')

  const kindLabel: Record<BackupKind, string> = {
    daily: 'Daily',
    manual: 'Made by you',
    preUpdate: 'Before an update',
    beforeRestore: 'Before a restore',
  }
  const shown = $derived(all ? (backups?.files ?? []) : (backups?.files ?? []).slice(0, 5))
  const size = (n: number) => (n < 1024 * 1024 ? `${Math.max(1, Math.round(n / 1024))} KB` : `${(n / 1048576).toFixed(1)} MB`)
  const when = (s: string | null) => (s ? new Date(s).toLocaleString() : 'never')

  onMount(load)

  async function load() {
    try {
      backups = await api.backups()
    } catch (e) {
      error = message(e)
    }
  }

  async function run(f: () => Promise<void>) {
    busy = true
    error = ''
    try {
      await f()
    } catch (e) {
      error = message(e)
    } finally {
      busy = false
    }
  }

  const backupNow = () =>
    run(async () => {
      const name = await api.backupNow()
      await load()
      app.say(`Backed up: ${name}.`)
    })

  const look = (name: string) =>
    run(async () => {
      chosen = { name, label: name, summary: null }
      chosen.summary = await api.inspectBackup(name, null)
    })

  const pickFile = () =>
    run(async () => {
      const label = await api.pickBackup()
      if (label) chosen = { name: null, label, summary: null }
      pass = ''
    })

  const unlockFile = () =>
    run(async () => {
      if (chosen) chosen.summary = await api.inspectBackup(null, pass)
    })

  const restore = () =>
    run(async () => {
      if (!chosen) return
      const saved = await api.restoreBackup(chosen.name, chosen.name ? null : pass)
      chosen = null
      pass = ''
      await load()
      await app.refresh()
      app.say(`Restored. Your data from before is kept as ${saved}.`)
    })

  const doExport = () =>
    run(async () => {
      if (exportPass !== exportPass2) throw 'The two passphrases are not the same.'
      if (await api.exportBackup(exportPass)) {
        exporting = false
        exportPass = exportPass2 = ''
        app.say('Exported. Keep the passphrase safe: the file cannot be opened without it.')
      }
    })
</script>

<h3>Backups</h3>
<p class="hint">
  Sachiva backs up once a day while it runs and keeps the last 14 daily backups. It also backs up before every update
  and every restore. Backups are encrypted with your recovery key.
</p>
{#if backups}
  <p class="hint">Folder: <code>{backups.folder}</code></p>
  {#if backups.files.length}
    <ul class="files">
      {#each shown as f (f.name)}
        <li>
          <span class="kind">{kindLabel[f.kind]}</span>
          <code>{f.name}</code>
          <span class="hint">{size(f.bytes)}</span>
          <button class="btn sm" disabled={busy} onclick={() => look(f.name)}>Restore…</button>
        </li>
      {/each}
    </ul>
    {#if backups.files.length > 5}
      <button class="btn sm" onclick={() => (all = !all)}>{all ? 'Show fewer' : `Show all ${backups.files.length}`}</button>
    {/if}
  {:else}
    <p class="hint">No backups yet.</p>
  {/if}
{/if}
<div class="row">
  <button class="btn" disabled={busy} onclick={backupNow}>Back up now</button>
  <button class="btn" disabled={busy} onclick={pickFile}>Restore from a file…</button>
  <button class="btn" disabled={busy} onclick={() => (exporting = !exporting)}>Export a copy…</button>
</div>

{#if chosen}
  <div class="box">
    <b>Restore {chosen.label}?</b>
    {#if !chosen.summary && !chosen.name}
      <p class="hint">
        Enter the passphrase chosen when this copy was exported. For a backup from another device, enter that device's
        recovery key.
      </p>
      <input type="password" bind:value={pass} placeholder="Passphrase or recovery key" autocomplete="off" />
      <div class="row">
        <button class="btn p" disabled={busy || !pass} onclick={unlockFile}>Open</button>
        <button class="btn" onclick={() => (chosen = null)}>Cancel</button>
      </div>
    {:else if chosen.summary}
      {@const s = chosen.summary}
      <p class="hint">
        It holds {s.openJobs} open {s.openJobs === 1 ? 'job' : 'jobs'}, {s.doneJobs} done and {s.trashed} in the trash.
        Last change: {when(s.lastChange)}.
      </p>
      <p class="hint">
        Restoring replaces all your current jobs with these. Your current data is backed up first, so you can undo this
        by restoring that backup.
      </p>
      <div class="row">
        <button class="btn p" disabled={busy} onclick={restore}>Restore</button>
        <button class="btn" onclick={() => (chosen = null)}>Cancel</button>
      </div>
    {:else}
      <p class="hint">Reading…</p>
    {/if}
  </div>
{/if}

{#if exporting}
  <div class="box">
    <b>Export a copy</b>
    <p class="hint">
      For moving your data to another device, or keeping a copy elsewhere. The copy is encrypted with a passphrase you
      choose now (at least 10 characters). Nobody can open it without the passphrase, not even you, so write it down.
    </p>
    <input type="password" bind:value={exportPass} placeholder="Passphrase" autocomplete="new-password" />
    <input type="password" bind:value={exportPass2} placeholder="The same passphrase again" autocomplete="new-password" />
    <div class="row">
      <button class="btn p" disabled={busy || exportPass.length < 10} onclick={doExport}>Choose where to save</button>
      <button class="btn" onclick={() => (exporting = false)}>Cancel</button>
    </div>
  </div>
{/if}
{#if error}<p class="error" role="alert">{error}</p>{/if}

<style>
  .hint {
    font-size: 12px;
    color: var(--muted);
  }
  .files {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
  }
  .files li {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
  }
  .kind {
    font-size: 12px;
    font-weight: 600;
    min-width: 120px;
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
  .sm {
    padding: 3px 10px;
    font-size: 12px;
  }
  .box {
    width: 100%;
    padding: 12px 14px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--panel2);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .box input {
    max-width: 360px;
  }
  .error {
    color: var(--urgent);
  }
</style>
