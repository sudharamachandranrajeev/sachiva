<script lang="ts">
  import { onMount } from 'svelte'
  import { api, message, type Finding } from './api'
  import { app } from './state.svelte'
  import { draftFromNote } from './noteCapture'
  import { fromInput, toInput } from './time'
  import type { Importance } from './api'

  // Quick capture: type a title, press Enter. Details can follow later.
  let title = $state('')
  let findings = $state<Finding[]>([])
  let dialog: HTMLDialogElement
  let noteMode = $state(false)
  let note = $state('')
  let reviewed = $state(false)
  let draftTitle = $state('')
  let due = $state<string | null>(null)
  let importance = $state<Importance>('normal')
  let assumedTime = $state(false)
  let busy = $state(false)

  onMount(() => dialog.showModal())

  $effect(() => {
    if (noteMode) return
    const text = title
    const h = setTimeout(async () => {
      findings = text.trim() ? await api.check(text).catch(() => []) : []
    }, 250)
    return () => clearTimeout(h)
  })

  async function preview() {
    const source = note
    const draft = draftFromNote(source)
    draftTitle = draft.title
    due = draft.due
    importance = draft.importance
    assumedTime = !!due && !draft.dateHadTime
    try {
      const checked = await api.check(source)
      if (note !== source || !noteMode) return
      findings = checked
      reviewed = true
    } catch (e) { app.say(message(e), { error: true }) }
  }

  $effect(() => {
    if (!noteMode || !reviewed) return
    const text = `${note}\n${draftTitle}`
    let active = true
    const h = setTimeout(async () => {
      try { const checked = await api.check(text); if (active) findings = checked } catch (e) { if (active) app.say(message(e), { error: true }) }
    }, 250)
    return () => { active = false; clearTimeout(h) }
  })

  async function addNote() {
    if (!draftTitle.trim() || busy) return
    busy = true
    try {
      const task = await api.newTask(draftTitle.trim())
      task.notes = note.trim()
      task.due = due
      task.importance = importance
      const saved = await api.saveTask(task)
      await app.refresh()
      app.view = 'dashboard'
      app.capturing = false
      app.say(`Added: ${saved.title}.`, { undo: true })
    } catch (e) { app.say(message(e), { error: true }) }
    finally { busy = false }
  }

  async function add(details: boolean) {
    const text = title.trim()
    if (!text) return
    try {
      const saved = await api.saveTask(await api.newTask(text))
      app.capturing = false
      if (details) app.edit(saved)
      else app.say(`Added: ${saved.title}.`, { undo: true })
    } catch (e) {
      app.say(message(e), { error: true })
    }
  }
</script>

<dialog
  bind:this={dialog}
  aria-label="Quick capture"
  oncancel={(e) => {
    e.preventDefault()
    app.capturing = false
  }}
>
  <div class="mode"><button class="btn" onclick={() => { noteMode = !noteMode; findings = []; reviewed = false }}>{noteMode ? 'Quick capture' : 'Create from a note'}</button></div>
  {#if noteMode}
    <label for="capture-note">Paste a note or message</label>
    <textarea id="capture-note" bind:value={note} maxlength="20000" rows="6" placeholder="Subject: Order supplies&#10;Deadline: Friday 2pm&#10;Priority: high&#10;Details…" oninput={() => { reviewed = false; findings = [] }}></textarea>
    <p class="muted hint">Processed on this device. Use reference codes, not patient details.</p>
    <button class="btn" onclick={preview} disabled={!note.trim()}>Extract details</button>
    {#if reviewed}
      <div class="preview">
        <p>Review the suggested details before adding the job. The full note is kept in its notes.</p>
        <label>Title <input bind:value={draftTitle} maxlength="300" /></label>
        <label>Deadline <input type="datetime-local" value={toInput(due)} onchange={(e) => { due = fromInput(e.currentTarget.value); assumedTime = false }} /></label>
        {#if assumedTime}<p class="muted hint">No time was given; 17:00 is suggested. Please check it.</p>{/if}
        <label>Priority <select bind:value={importance}><option value="low">Low</option><option value="normal">Normal</option><option value="high">High</option></select></label>
      </div>
      {#if findings.length}<div class="check" role="status">{#each findings as f, i (i)}<div>“{f.text}”. {f.message}</div>{/each}</div>{/if}
      <div class="row"><button class="btn p" onclick={addNote} disabled={!draftTitle.trim() || busy}>Add to dashboard</button></div>
    {/if}
  {:else}
  <!-- svelte-ignore a11y_autofocus -->
  <input
    bind:value={title}
    autofocus
    maxlength="300"
    placeholder="What needs doing? Press Enter to add."
    onkeydown={(e) => {
      if (e.key === 'Enter') {
        e.preventDefault()
        add(e.shiftKey)
      }
    }}
  />
  {#if findings.length}
    <div class="check" role="status">
      {#each findings as f, i (i)}<div>“{f.text}”. {f.message}</div>{/each}
    </div>
  {/if}
  <div class="row">
    <span class="muted">Enter adds it. Shift + Enter adds it and opens the details.</span>
    <button class="btn" onclick={() => add(true)} disabled={!title.trim()}>Add with details</button>
    <button class="btn p" onclick={() => add(false)} disabled={!title.trim()}>Add</button>
  </div>
  {/if}
</dialog>

<style>
  dialog {
    margin: 12vh auto auto;
    width: min(640px, calc(100vw - 32px));
    padding: 14px;
    border: 1px solid var(--line);
    border-radius: var(--r);
    background: var(--panel);
    color: var(--ink);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.25);
    animation: rise 0.18s ease-out;
  }
  dialog::backdrop {
    background: rgba(10, 25, 22, 0.35);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }
  }
  .mode { margin-bottom: 12px; }
  label { display: block; margin: 10px 0 4px; }
  textarea, select { width: 100%; }
  .preview { margin-top: 14px; }
  .hint { font-size: 12px; margin: 6px 0; }
  dialog { max-height: 85vh; overflow-y: auto; }
  input {
    font-size: 17px;
    padding: 12px 14px;
  }
  .check {
    margin-top: 10px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--warn-soft);
    border-left: 4px solid var(--warn);
    font-size: 13px;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 10px;
    flex-wrap: wrap;
  }
  .row .muted {
    flex: 1;
    font-size: 12px;
  }
</style>
