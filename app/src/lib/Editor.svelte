<script lang="ts">
  import { onMount } from 'svelte'
  import { api, message, type Finding, type Repeat, type Task, type Weekday } from './api'
  import { app } from './state.svelte'
  import { fromInput, minutesToTomorrowMorning, toInput, when } from './time'

  let { task: initial, isNew }: { task: Task; isNew: boolean } = $props()

  // The editor works on its own copy and saves it shortly after each change.
  // svelte-ignore state_referenced_locally
  let t = $state<Task>($state.snapshot(initial))
  // svelte-ignore state_referenced_locally
  let stored = $state(!isNew)
  let saving: Promise<void> | null = null
  let timer: ReturnType<typeof setTimeout> | undefined
  let saveState = $state<'idle' | 'saving' | 'saved' | 'error'>('idle')
  let error = $state('')
  let findings = $state<{ field: string; f: Finding }[]>([])
  let dialog: HTMLDialogElement
  // svelte-ignore state_referenced_locally
  let tagText = $state(initial.tags.join(', '))
  let newItem = $state('')

  const days: [Weekday, string][] = [
    ['mon', 'Mon'],
    ['tue', 'Tue'],
    ['wed', 'Wed'],
    ['thu', 'Thu'],
    ['fri', 'Fri'],
    ['sat', 'Sat'],
    ['sun', 'Sun'],
  ]

  onMount(() => {
    dialog.showModal()
  })

  let first = true
  $effect(() => {
    $state.snapshot(t)
    if (first) {
      first = false
      return
    }
    clearTimeout(timer)
    timer = setTimeout(save, 600)
  })

  async function save() {
    clearTimeout(timer)
    timer = undefined
    if (!t.title.trim()) {
      saveState = 'idle'
      return
    }
    saveState = 'saving'
    const run = api.saveTask($state.snapshot(t)).then(
      () => {
        stored = true
        saveState = 'saved'
        error = ''
      },
      (e) => {
        saveState = 'error'
        error = message(e)
      },
    )
    saving = run
    await run
  }

  /** Wait for any pending save. False if it failed, so the editor stays open. */
  async function flush(): Promise<boolean> {
    if (timer) await save()
    else if (saving) await saving
    return saveState !== 'error'
  }

  // The checker runs on every text field as you type. It warns; it never blocks.
  $effect(() => {
    const fields: [string, string][] = [
      ['Title', t.title],
      ['Notes', t.notes],
      ['Site', t.site],
      ['Tags', tagText],
      ...t.checklist.map((c, i): [string, string] => [`Checklist item ${i + 1}`, c.text]),
    ]
    if (t.status.state === 'waiting') fields.push(['Waiting on', t.status.on])
    const h = setTimeout(async () => {
      try {
        const found = await Promise.all(
          fields.filter(([, text]) => text.trim()).map(async ([field, text]) => (await api.check(text)).map((f) => ({ field, f }))),
        )
        findings = found.flat()
      } catch {
        findings = []
      }
    }, 300)
    return () => clearTimeout(h)
  })

  async function close() {
    if (!(await flush())) return
    app.editing = null
  }

  async function done() {
    if (!(await flush())) return
    app.editing = null
    await app.act(() => api.complete(t.id), `Done: ${t.title}.`)
  }

  async function remove() {
    clearTimeout(timer)
    timer = undefined
    if (saving) await saving
    app.editing = null
    if (stored) await app.act(() => api.deleteTask(t.id), `Moved to the trash: ${t.title}.`)
  }

  function quiet(minutes: number) {
    t.snoozedUntil = new Date(Date.now() + minutes * 60_000).toISOString()
  }

  function setTags(v: string) {
    tagText = v
    t.tags = [...new Set(v.split(',').map((s) => s.trim()).filter(Boolean))]
  }

  function setRepeat(kind: string) {
    const due = t.due ? new Date(t.due) : new Date()
    const r: Record<string, Repeat | null> = {
      none: null,
      daily: { kind: 'daily', every: 1 },
      weekly: { kind: 'weekly', every: 1, days: [] },
      monthly: { kind: 'monthly', every: 1, day: due.getDate() },
      afterDone: { kind: 'afterDone', days: 7 },
    }
    t.repeat = r[kind] ?? null
  }

  function toggleDay(d: Weekday) {
    if (t.repeat?.kind !== 'weekly') return
    const has = t.repeat.days.includes(d)
    t.repeat.days = has ? t.repeat.days.filter((x) => x !== d) : [...t.repeat.days, d]
  }

  function addItem() {
    const text = newItem.trim()
    if (!text) return
    t.checklist.push({ text, done: false })
    newItem = ''
  }

  const whole = (v: unknown, min: number, max: number) => Math.min(max, Math.max(min, Math.round(Number(v) || min)))
  const snoozed = $derived(t.snoozedUntil && new Date(t.snoozedUntil) > new Date() ? t.snoozedUntil : null)
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="editor-title"
  oncancel={(e) => {
    e.preventDefault()
    close()
  }}
>
  <header>
    <h2 id="editor-title">{isNew && !stored ? 'New job' : 'Job'}</h2>
    <span class="saved muted" aria-live="polite">
      {#if saveState === 'saving'}Saving…{:else if saveState === 'saved'}Saved{:else if saveState === 'error'}Not saved{:else if !t.title.trim()}Saved once it has a title{/if}
    </span>
    <button class="btn" onclick={close}>Close</button>
  </header>

  <div class="body">
    <label class="field">
      Title
      <!-- svelte-ignore a11y_autofocus -->
      <input
        bind:value={t.title}
        maxlength="300"
        placeholder="e.g. Chase PO-0419 dressings delivery"
        autofocus={isNew}
      />
    </label>

    {#if findings.length}
      <div class="check" role="status">
        <b>Check before you keep this.</b> Sachiva is for reference codes, not patient details.
        <ul>
          {#each findings as { field, f }, i (i)}
            <li><span class="muted">{field}:</span> “{f.text}”. {f.message}</li>
          {/each}
        </ul>
        <span class="muted small">The checker can miss things, and can flag things that are fine. It never blocks saving.</span>
      </div>
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}

    <div class="row2">
      <label class="field">
        Due by
        <input type="datetime-local" bind:value={() => toInput(t.due), (v) => (t.due = fromInput(v))} />
      </label>
      <label class="field">
        Appointment time
        <input
          type="datetime-local"
          bind:value={() => toInput(t.appointment), (v) => (t.appointment = fromInput(v))}
        />
      </label>
    </div>

    <div class="row2">
      <label class="field">
        Work needed (minutes)
        <input
          type="number"
          min="0"
          max="10080"
          step="5"
          placeholder="e.g. 30"
          bind:value={
            () => t.effortMin ?? '',
            (v) => (t.effortMin = v === '' || v == null ? null : whole(v, 0, 10080))
          }
        />
      </label>
      <div class="field-group">
        <span class="label">Importance</span>
        <div class="seg" role="radiogroup" aria-label="Importance">
          {#each [['low', 'Low'], ['normal', 'Normal'], ['high', 'Important']] as [v, name] (v)}
            <button
              role="radio"
              aria-checked={t.importance === v}
              class:sel={t.importance === v}
              onclick={() => (t.importance = v as Task['importance'])}>{name}</button
            >
          {/each}
        </div>
      </div>
    </div>

    <div class="row2">
      <label class="field">
        Site
        <input bind:value={t.site} list="sites" maxlength="300" placeholder="e.g. Site A" />
        <datalist id="sites">{#each app.sites as s (s)}<option value={s}></option>{/each}</datalist>
      </label>
      <label class="field">
        Tags, separated by commas
        <input value={tagText} oninput={(e) => setTags(e.currentTarget.value)} placeholder="e.g. orders, month end" />
      </label>
    </div>

    <div class="field-group">
      <span class="label">Status</span>
      <div class="seg" role="radiogroup" aria-label="Status">
        <button
          role="radio"
          aria-checked={t.status.state !== 'waiting'}
          class:sel={t.status.state !== 'waiting'}
          onclick={() => (t.status = { state: 'open' })}>Mine to do</button
        >
        <button
          role="radio"
          aria-checked={t.status.state === 'waiting'}
          class:sel={t.status.state === 'waiting'}
          onclick={() => {
            if (t.status.state !== 'waiting') t.status = { state: 'waiting', on: '', chase: null }
          }}>Waiting on someone</button
        >
      </div>
      {#if t.status.state === 'waiting'}
        {@const w = t.status}
        <div class="row2">
          <label class="field">
            Waiting on
            <input bind:value={w.on} maxlength="300" placeholder="e.g. supplier, estates, manager" />
          </label>
          <label class="field">
            Chase them on
            <input type="datetime-local" bind:value={() => toInput(w.chase), (v) => (w.chase = fromInput(v))} />
          </label>
        </div>
      {/if}
    </div>

    <label class="check-row">
      <input type="checkbox" bind:checked={t.pinned} />
      Pin to the top of Today
    </label>

    <div class="field-group">
      <span class="label">Checklist</span>
      {#each t.checklist as item, i (i)}
        <div class="item">
          <input type="checkbox" bind:checked={item.done} aria-label="Done" />
          <input bind:value={item.text} maxlength="300" aria-label="Checklist item {i + 1}" />
          <button class="btn" aria-label="Remove item" onclick={() => t.checklist.splice(i, 1)}>×</button>
        </div>
      {/each}
      <div class="item">
        <span></span>
        <input
          bind:value={newItem}
          placeholder="Add a step and press Enter"
          onkeydown={(e) => e.key === 'Enter' && (e.preventDefault(), addItem())}
        />
        <button class="btn" onclick={addItem}>Add</button>
      </div>
    </div>

    <div class="field-group">
      <span class="label">Repeat</span>
      <select value={t.repeat?.kind ?? 'none'} onchange={(e) => setRepeat(e.currentTarget.value)} aria-label="Repeat">
        <option value="none">Does not repeat</option>
        <option value="daily">Every few days, on a schedule</option>
        <option value="weekly">Weekly</option>
        <option value="monthly">Monthly</option>
        <option value="afterDone">Some days after it is done</option>
      </select>
      {#if t.repeat?.kind === 'daily'}
        {@const r = t.repeat}
        <label class="inline">Every <input type="number" min="1" max="366" bind:value={() => r.every, (v) => (r.every = whole(v, 1, 366))} /> days</label>
      {:else if t.repeat?.kind === 'weekly'}
        {@const r = t.repeat}
        <label class="inline">Every <input type="number" min="1" max="366" bind:value={() => r.every, (v) => (r.every = whole(v, 1, 366))} /> weeks, on</label>
        <div class="days">
          {#each days as [d, name] (d)}
            <button class:sel={r.days.includes(d)} aria-pressed={r.days.includes(d)} onclick={() => toggleDay(d)}>{name}</button>
          {/each}
        </div>
        {#if !r.days.length}<span class="muted small">No days picked: it repeats on the due date's weekday.</span>{/if}
      {:else if t.repeat?.kind === 'monthly'}
        {@const r = t.repeat}
        <label class="inline">
          Every <input type="number" min="1" max="366" bind:value={() => r.every, (v) => (r.every = whole(v, 1, 366))} /> months,
          on day <input type="number" min="1" max="31" bind:value={() => r.day, (v) => (r.day = whole(v, 1, 31))} />
        </label>
        {#if r.day > 28}<span class="muted small">In shorter months it falls on the last day.</span>{/if}
      {:else if t.repeat?.kind === 'afterDone'}
        {@const r = t.repeat}
        <label class="inline"><input type="number" min="1" max="366" bind:value={() => r.days, (v) => (r.days = whole(v, 1, 366))} /> days after you finish it</label>
      {/if}
      {#if t.repeat}<span class="muted small">When you mark it done, the next one is made with the same times moved on.</span>{/if}
    </div>

    <label class="field">
      Notes
      <textarea bind:value={t.notes} maxlength="20000" placeholder="Reference codes and steps only, no patient details."></textarea>
    </label>

    <div class="field-group">
      <span class="label">Reminders</span>
      {#if snoozed}
        <span>Quiet until {when(snoozed)}. <button class="link" onclick={() => (t.snoozedUntil = null)}>Turn back on</button></span>
      {:else}
        <div class="btns">
          <button class="btn" onclick={() => quiet(15)}>Quiet 15 min</button>
          <button class="btn" onclick={() => quiet(60)}>Quiet 1 hour</button>
          <button class="btn" onclick={() => quiet(minutesToTomorrowMorning())}>Quiet until 9:00 tomorrow</button>
        </div>
      {/if}
    </div>
  </div>

  <footer>
    <button class="btn danger" onclick={remove}>{stored ? 'Delete' : 'Discard'}</button>
    <span class="sp"></span>
    <button class="btn p" disabled={!t.title.trim()} onclick={done}>Mark done</button>
  </footer>
</dialog>

<style>
  dialog {
    margin: auto;
    width: min(720px, calc(100vw - 32px));
    max-height: calc(100vh - 48px);
    border: 1px solid var(--line);
    border-radius: var(--r);
    background: var(--panel);
    color: var(--ink);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    animation: rise 0.2s ease-out;
  }
  dialog::backdrop {
    background: rgba(10, 25, 22, 0.35);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  header,
  footer {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 20px;
    border-bottom: 1px solid var(--line);
  }
  footer {
    border-bottom: 0;
    border-top: 1px solid var(--line);
  }
  header h2 {
    font-size: 17px;
  }
  .saved {
    flex: 1;
    font-size: 12px;
  }
  .sp {
    flex: 1;
  }
  .body {
    padding: 16px 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .row2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .field-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .label {
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
  }
  .seg {
    display: flex;
    border: 1px solid var(--line);
    border-radius: 10px;
    overflow: hidden;
    width: fit-content;
  }
  .seg button {
    padding: 8px 12px;
    font-size: 13px;
    background: var(--panel2);
    border-right: 1px solid var(--line);
  }
  .seg button:last-child {
    border-right: 0;
  }
  .seg button.sel {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .check {
    padding: 12px 14px;
    border-radius: 12px;
    background: var(--warn-soft);
    border-left: 4px solid var(--warn);
    font-size: 13px;
  }
  .check ul {
    margin: 6px 0 6px 18px;
  }
  .small {
    font-size: 12px;
  }
  .error {
    color: var(--urgent);
    font-weight: 600;
  }
  .check-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .item {
    display: grid;
    grid-template-columns: 20px 1fr auto;
    gap: 8px;
    align-items: center;
  }
  .inline {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .inline input {
    width: 80px;
  }
  .days {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .days button {
    padding: 6px 10px;
    border-radius: 999px;
    border: 1px solid var(--line);
    font-size: 12px;
  }
  .days button.sel {
    background: var(--accent);
    color: var(--on-accent);
    border-color: var(--accent);
  }
  .btns {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .link {
    color: var(--accent);
    font-weight: 600;
    text-decoration: underline;
  }
  @media (max-width: 760px) {
    dialog {
      width: 100vw;
      max-width: 100vw;
      height: 100%;
      max-height: 100%;
      border-radius: 0;
      border: 0;
    }
    .row2 {
      grid-template-columns: 1fr;
    }
  }
</style>
