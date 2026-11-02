<script lang="ts">
  import { onMount } from 'svelte'
  import { api, message, type Finding } from './api'
  import { parseCalendar, type CalendarDraft } from './calendarImport'
  import { app } from './state.svelte'

  let dialog: HTMLDialogElement
  let entries = $state<CalendarDraft[]>([])
  let chosen = $state<string[]>([])
  let skipped = $state<string[]>([])
  let findings = $state<{ title: string; finding: Finding }[]>([])
  let ready = $state(false)
  let busy = $state(false)
  let error = $state('')
  let fileName = $state('')
  let sequence = 0
  onMount(() => dialog.showModal())
  function close() { sequence++; app.importingCalendar = false }
  async function readFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file) return
    const request = ++sequence
    entries = []; chosen = []; skipped = []; findings = []; ready = false; error = ''; fileName = file.name
    if (file.size > 2_000_000) { error = 'Choose a calendar file under 2 MB.'; return }
    try {
      const preview = parseCalendar(await file.text())
      const found = (await Promise.all(preview.entries.map(async (e) => (await api.check(`${e.title}\n${e.notes}`)).map((finding) => ({ title: e.title, finding }))))).flat()
      if (request !== sequence) return
      entries = preview.entries; skipped = preview.skipped; findings = found
      chosen = preview.entries.map((e) => e.uid); ready = true
    } catch (e) { if (request === sequence) error = message(e) }
  }
  async function add() {
    if (busy || !ready || !chosen.length) return
    busy = true
    try {
      const selected = entries.filter((entry) => chosen.includes(entry.uid))
      const result = await api.importCalendar($state.snapshot(selected))
      await app.refresh()
      app.view = 'dashboard'
      close()
      app.say(`Imported ${result.imported} ${result.imported === 1 ? 'job' : 'jobs'}; ${result.duplicates} already imported.`, { undo: result.imported > 0 })
    } catch (e) { error = message(e) }
    finally { busy = false }
  }
</script>
<dialog bind:this={dialog} aria-labelledby="calendar-title" oncancel={(e) => { e.preventDefault(); if (!busy) close() }}>
  <h2 id="calendar-title">Import calendar</h2>
  <p>Choose an .ics file. Events become appointments and calendar tasks with due dates become deadline jobs.</p>
  <p class="muted hint">The file stays on this device. Review entries and use reference codes, not patient details.</p>
  <label>Calendar file <input type="file" accept=".ics,text/calendar" onchange={readFile} disabled={busy} /></label>
  {#if fileName}<p class="hint">{fileName}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if findings.length}
    <div class="check" role="status"><b>Check for confidential information</b><p>Uncheck an entry to leave it out, or remove the information from the file and choose it again.</p>
      {#each findings as row, i (i)}<div><b>{row.title}:</b> “{row.finding.text}”. {row.finding.message}</div>{/each}
    </div>
  {/if}
  {#if ready}
    <p>Choose the entries to import. All-day dates suggest 17:00 local; change them after import if needed.</p>
    <div class="entries">
      {#each entries as e (e.uid)}
        <label class="entry"><input type="checkbox" value={e.uid} bind:group={chosen} disabled={busy} />
          <span><b>{e.title}</b><br /><span class="muted">{e.kind === 'appointment' ? 'Appointment' : 'Deadline'} · {new Date(e.when).toLocaleString()}{e.allDay ? ' · all-day, 17:00 suggested' : ''}</span>{#if e.notes}<br /><span class="muted notes">{e.notes}</span>{/if}</span>
        </label>
      {:else}<p class="muted">No entries can be imported from this file.</p>{/each}
    </div>
    {#if skipped.length}<details><summary>{skipped.length} skipped entries</summary><ul>{#each skipped as reason}<li>{reason}</li>{/each}</ul></details>{/if}
  {/if}
  <div class="row"><button class="btn" onclick={close} disabled={busy}>Cancel</button><button class="btn p" onclick={add} disabled={busy || !ready || !chosen.length}>Import {chosen.length || ''}</button></div>
</dialog>
<style>
  dialog { margin: auto; width: min(700px, calc(100vw - 32px)); max-height: 85vh; overflow-y: auto; padding: 20px; border: 1px solid var(--line); border-radius: var(--r); background: var(--panel); color: var(--ink); }
  dialog::backdrop { background: rgba(10, 25, 22, 0.4); }
  p { margin: 8px 0; }
  .hint { font-size: 12px; }
  .entries { display: grid; gap: 8px; }
  .entry { display: flex; align-items: start; gap: 10px; padding: 10px; border: 1px solid var(--line); border-radius: 10px; }
  .entry input { width: auto; margin-top: 4px; }
  .notes { white-space: pre-wrap; overflow-wrap: anywhere; }
  .check { margin: 10px 0; padding: 10px; background: var(--warn-soft); border-left: 4px solid var(--warn); }
  .error { color: var(--urgent); }
  .row { display: flex; gap: 8px; justify-content: end; margin-top: 16px; }
</style>
