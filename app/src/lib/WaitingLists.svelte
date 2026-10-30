<script lang="ts">
  import { api, message, type Figures, type Finding, type Span, type WaitList } from './api'
  import { app } from './state.svelte'
  import { isoDay, periodLabel } from './time'

  let lists = $state<WaitList[]>([])
  let showArchived = $state(false)
  let sel = $state<string | null>(null)
  /** The list being edited: a copy, saved with "Save list". */
  let edit = $state<WaitList | null>(null)
  let bandText = $state('')
  let newName = $state('')
  let error = $state('')
  let findings = $state<{ field: string; f: Finding }[]>([])

  let span = $state<Span>('week')
  let day = $state(isoDay(new Date()))
  let fig = $state<Figures | null>(null)
  let figError = $state('')
  let saving = $state(false)

  const shown = $derived(lists.filter((l) => showArchived || !l.archived))
  const current = $derived(lists.find((l) => l.id === sel) ?? null)

  async function load() {
    try {
      lists = await api.waitlists()
      if (!sel || !lists.some((l) => l.id === sel)) pick(lists.find((l) => !l.archived) ?? null)
    } catch (e) {
      error = message(e)
    }
  }
  $effect(() => {
    void app.version
    load()
  })

  function pick(l: WaitList | null) {
    sel = l?.id ?? null
    edit = l ? $state.snapshot(l) : null
    bandText = l ? l.bands.join(', ') : ''
    error = ''
  }

  async function create() {
    const name = newName.trim()
    if (!name) return
    try {
      const l = await api.saveWaitlist(await api.newWaitlist(name))
      newName = ''
      lists = [...lists, l]
      pick(l)
      app.say(`Made the list ${l.name}.`)
    } catch (e) {
      error = message(e)
    }
  }

  /** "9, 18, 52" to [9, 18, 52]; null when it does not read as whole numbers. */
  function parseBands(s: string): number[] | null {
    const parts = s.split(/[\s,]+/).filter(Boolean)
    const v = parts.map(Number)
    return parts.length && v.every((n) => Number.isInteger(n) && n > 0) ? v : null
  }

  async function saveList() {
    if (!edit) return
    const bands = parseBands(bandText)
    if (!bands) return (error = 'Write the band edges as whole weeks, such as 9, 18, 52.')
    const tw = edit.targetWeeks as number | null | undefined
    if (tw != null && !Number.isInteger(tw)) return (error = 'The target wait is a whole number of weeks.')
    error = ''
    try {
      const saved = await api.saveWaitlist({ ...$state.snapshot(edit), bands, targetWeeks: tw ?? null })
      lists = lists.map((l) => (l.id === saved.id ? saved : l))
      pick(saved)
      app.say(`Saved ${saved.name}.`)
    } catch (e) {
      error = message(e)
    }
  }

  // The checker warns about patient details in names; it never blocks.
  $effect(() => {
    const e = edit
    if (!e) {
      findings = []
      return
    }
    const fields: [string, string][] = [
      ['Name', e.name],
      ['Site', e.site],
      ['Clinic', e.clinic],
      ['Provider', e.provider],
      ['Note', fig?.note ?? ''],
    ]
    const h = setTimeout(async () => {
      try {
        const found = await Promise.all(
          fields.filter(([, t]) => t.trim()).map(async ([field, t]) => (await api.check(t)).map((f) => ({ field, f }))),
        )
        findings = found.flat()
      } catch {
        findings = []
      }
    }, 300)
    return () => clearTimeout(h)
  })

  // Load the figures for the chosen list and period.
  $effect(() => {
    const id = sel
    const s = span
    const d = day
    void app.version
    fig = null
    figError = ''
    if (!id || !d) return
    api.getFigures(id, s, d).then(
      (f) => {
        if (sel === id && span === s && day === d) fig = f
      },
      (e) => (figError = message(e)),
    )
  })

  const fields: [keyof Figures & ('total' | 'added' | 'removed' | 'booked' | 'longestWeeks'), string, string][] = [
    ['total', 'Total waiting', 'At the end of the period.'],
    ['added', 'Added', 'Joined the list in the period.'],
    ['removed', 'Removed', 'Left the list for any reason.'],
    ['booked', 'Booked', 'Given an appointment from the list.'],
    ['longestWeeks', 'Longest wait (weeks)', 'The person waiting longest, in whole weeks.'],
  ]

  /** A blank box is "not entered" (null), never 0. */
  const clean = (v: unknown): number | null => (v === '' || v === undefined || v === null ? null : (v as number))
  const bad = (v: number | null) => v !== null && (!Number.isInteger(v) || v < 0)

  const bandSum = $derived.by(() => {
    if (!fig || fig.bands.some((b) => clean(b) === null)) return null
    return fig.bands.reduce<number>((a, b) => a + (b ?? 0), 0)
  })

  async function saveFigures() {
    if (!fig) return
    const f: Figures = $state.snapshot(fig) as Figures
    for (const k of ['total', 'added', 'removed', 'booked', 'longestWeeks'] as const) f[k] = clean(f[k])
    f.bands = f.bands.map(clean)
    if ([f.total, f.added, f.removed, f.booked, f.longestWeeks, ...f.bands].some(bad)) {
      return (figError = 'Use whole numbers, 0 or more. Leave a box empty if you do not know it.')
    }
    figError = ''
    saving = true
    try {
      await api.saveFigures(f)
      app.say(`Saved figures for ${current?.name}, ${periodLabel(f.span, f.start)}.`)
    } catch (e) {
      figError = message(e)
    } finally {
      saving = false
    }
  }

  function bandNames(edges: number[]): string[] {
    let low = 0
    const v = edges.map((b) => {
      const s = `${low}-${b} weeks`
      low = b
      return s
    })
    return [...v, `over ${low} weeks`]
  }
</script>

<div class="top">
  <div>
    <h1>Waiting lists</h1>
    <div class="muted sub">Enter each list's figures by day, week or month. Reports chart them for you.</div>
  </div>
</div>

<div class="grid">
  <aside class="card side">
    <h3>Lists</h3>
    {#each shown as l (l.id)}
      <button class="pick" class:sel={l.id === sel} aria-current={l.id === sel ? 'true' : undefined} onclick={() => pick(l)}>
        <span>{l.name}</span>
        <span class="muted small">{[l.site, l.clinic, l.provider].filter(Boolean).join(' · ') || ' '}</span>
        {#if l.archived}<span class="muted small">Archived</span>{/if}
      </button>
    {:else}
      <p class="muted small">No lists yet. Name your first one below.</p>
    {/each}
    <form
      class="new"
      onsubmit={(e) => {
        e.preventDefault()
        create()
      }}
    >
      <input placeholder="New list, such as Clinic A follow-ups" aria-label="New list name" maxlength="200" bind:value={newName} />
      <button class="btn p" disabled={!newName.trim()}>Add list</button>
    </form>
    {#if lists.some((l) => l.archived)}
      <label class="check-row small"><input type="checkbox" bind:checked={showArchived} /> Show archived lists</label>
    {/if}
  </aside>

  <section>
    {#if edit && current}
      <div class="card panel">
        <h2>Figures</h2>
        <div class="row">
          <label
            >Period
            <select bind:value={span}>
              <option value="day">Day</option>
              <option value="week">Week (Monday to Sunday)</option>
              <option value="month">Month</option>
            </select>
          </label>
          <label
            >Any day in it
            <input type="date" bind:value={day} />
          </label>
          {#if fig}<div class="period"><b>{periodLabel(fig.span, fig.start)}</b></div>{/if}
        </div>

        {#if fig}
          <p class="muted small">Leave a box empty if you do not have the figure. Empty is shown as "not entered", never as 0.</p>
          <div class="figs">
            {#each fields as [k, name, hint] (k)}
              <label title={hint}
                >{name}
                <input type="number" min="0" step="1" inputmode="numeric" bind:value={fig[k]} />
              </label>
            {/each}
          </div>
          <h3>Waiting by length of wait</h3>
          <div class="figs">
            {#each bandNames(current.bands) as name, i (i)}
              <label>{name} <input type="number" min="0" step="1" inputmode="numeric" bind:value={fig.bands[i]} /></label>
            {/each}
          </div>
          {#if bandSum !== null && clean(fig.total) !== null && bandSum !== fig.total}
            <p class="warn small">
              The bands add up to {bandSum}, but the total is {fig.total}.{bandSum > (fig.total ?? 0)
                ? ' The bands cannot be more than the total.'
                : ''}
            </p>
          {/if}
          <label class="note">Note <input maxlength="500" placeholder="Anything that explains a change" bind:value={fig.note} /></label>
          <div class="btns">
            <button class="btn p" disabled={saving} onclick={saveFigures}>Save figures</button>
          </div>
        {/if}
        {#if figError}<p class="error">{figError}</p>{/if}
      </div>

      <div class="card panel">
        <h2>About this list</h2>
        <div class="figs wide">
          <label>Name <input maxlength="200" bind:value={edit.name} /></label>
          <label>Site <input maxlength="200" bind:value={edit.site} /></label>
          <label>Clinic <input maxlength="200" bind:value={edit.clinic} /></label>
          <label title="Initials or a role, not a full name"
            >Provider (initials or role) <input maxlength="200" bind:value={edit.provider} /></label
          >
          <label title="Upper edges of each band, in weeks"
            >Band edges in weeks <input placeholder="9, 18, 52" bind:value={bandText} /></label
          >
          <label>Target: longest wait in weeks <input type="number" min="1" max="520" bind:value={edit.targetWeeks} /></label>
        </div>
        <label class="check-row"><input type="checkbox" bind:checked={edit.archived} /> Archived (hidden here; its figures stay in reports)</label>
        {#if findings.length}
          <div class="check" role="status">
            <b>Check before you keep this. Sachiva is for reference codes, not patient details.</b>
            <ul>
              {#each findings as { field, f }, i (i)}<li>{field}: “{f.text}”. {f.message}</li>{/each}
            </ul>
          </div>
        {/if}
        <div class="btns">
          <button class="btn p" onclick={saveList}>Save list</button>
        </div>
        {#if error}<p class="error">{error}</p>{/if}
      </div>
    {:else}
      {#if error}<p class="error">{error}</p>{/if}
      <div class="card panel muted">Pick a list, or add one, to enter its figures.</div>
    {/if}

    <details class="explain">
      <summary>What is this page?</summary>
      <p>
        Keep the numbers for each waiting list: how many are waiting, how many joined and left, how many were booked, and
        how long people have waited, split into bands. Enter them daily, weekly or monthly, whichever you report on. Saving
        again for the same period keeps the old figures too; reports use the newest. Lists are archived, never deleted, so
        old reports still add up. Band edges cannot change once a list has figures; make a new list instead. Use initials
        or a role for the provider, never patient details.
      </p>
    </details>
  </section>
</div>

<style>
  .sub {
    margin-top: 4px;
  }
  .grid {
    display: grid;
    grid-template-columns: 280px 1fr;
    gap: 20px;
    margin-top: 18px;
    align-items: start;
  }
  .side {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .side h3 {
    font-size: 13px;
    color: var(--muted);
    margin: 0 4px 4px;
  }
  .pick {
    text-align: left;
    padding: 8px 10px;
    border-radius: 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-wrap: anywhere;
  }
  .pick.sel,
  .pick:hover {
    background: var(--panel2);
  }
  .pick.sel {
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .new {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 8px;
  }
  .panel {
    padding: 16px 18px;
    margin-bottom: 14px;
  }
  .panel h2 {
    font-size: 17px;
    margin-bottom: 10px;
  }
  .panel h3 {
    font-size: 13px;
    color: var(--muted);
    margin: 14px 0 6px;
  }
  .row {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    align-items: end;
  }
  .row select,
  .row input {
    width: auto;
  }
  .period {
    padding-bottom: 8px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 13px;
  }
  .figs {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 10px;
    margin: 8px 0;
  }
  .figs.wide {
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  }
  .note {
    margin-top: 8px;
  }
  .check-row {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }
  .check-row input {
    width: auto;
  }
  .btns {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }
  .small {
    font-size: 12px;
  }
  .warn {
    color: var(--warn);
  }
  .error {
    color: var(--urgent);
    margin-top: 10px;
  }
  .check {
    margin-top: 10px;
    padding: 10px 12px;
    border-left: 3px solid var(--warn);
    font-size: 13px;
  }
  .check ul {
    margin: 6px 0 0 18px;
  }
  @media (max-width: 860px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
