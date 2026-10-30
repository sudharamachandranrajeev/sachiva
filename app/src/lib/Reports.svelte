<script lang="ts">
  import { api, message, type Cell, type ExportFormat, type Preview, type ReportKind, type ReportRequest, type Span, type WaitList } from './api'
  import { app } from './state.svelte'
  import { isoDay } from './time'

  let kind = $state<ReportKind>('performance')
  let span = $state<Span>('week')
  let from = $state(defaultFrom('week'))
  let to = $state(isoDay(new Date()))
  let activities = $state<string[]>([])
  let listIds = $state<string[]>([])
  let lists = $state<WaitList[]>([])
  let preview = $state<Preview | null>(null)
  let error = $state('')
  let busy = $state<string | null>(null)

  /** The last 30 days, 12 weeks or 12 months up to `to`. */
  function defaultFrom(s: Span): string {
    const d = new Date()
    if (s === 'day') d.setDate(d.getDate() - 29)
    else if (s === 'week') d.setDate(d.getDate() - 7 * 11)
    else d.setMonth(d.getMonth() - 11, 1)
    return isoDay(d)
  }

  // Activity names from the targets and tallies on the dashboard.
  const activityNames = $derived(
    [...new Set(app.counters.map((c) => c.task.activity.trim() || c.task.title.trim()))].sort((a, b) => a.localeCompare(b)),
  )
  const openLists = $derived(lists.filter((l) => !l.archived || listIds.includes(l.id)))

  $effect(() => {
    void app.version
    api.waitlists().then(
      (l) => (lists = l),
      (e) => (error = message(e)),
    )
  })

  const request = (): ReportRequest => ({
    kind,
    span,
    from,
    to,
    activities: kind === 'performance' ? $state.snapshot(activities) : [],
    lists: kind === 'waitingList' ? $state.snapshot(listIds) : [],
  })

  function ready(): boolean {
    error = !from || !to ? 'Pick a start and an end date.' : from > to ? 'The start comes after the end.' : ''
    return !error
  }

  async function show() {
    if (!ready()) return
    busy = 'preview'
    try {
      preview = await api.previewReport(request())
    } catch (e) {
      error = message(e)
    } finally {
      busy = null
    }
  }

  const formats: [ExportFormat, string][] = [
    ['xlsx', 'Spreadsheet (.xlsx)'],
    ['docx', 'Document (.docx)'],
    ['pptx', 'Slides (.pptx)'],
    ['pdf', 'PDF (.pdf)'],
  ]

  async function save(format: ExportFormat, name: string) {
    if (!ready()) return
    busy = format
    try {
      if (await api.exportReport(request(), format)) app.say(`Saved the report: ${name}.`)
    } catch (e) {
      error = message(e)
    } finally {
      busy = null
    }
  }

  function toggle(v: string[], x: string): string[] {
    return v.includes(x) ? v.filter((y) => y !== x) : [...v, x]
  }

  const svgUrl = (svg: string) => `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`
  const cell = (c: Cell) => (c === null ? '–' : typeof c === 'number' ? (Number.isInteger(c) ? String(c) : c.toFixed(1)) : c)
</script>

<div class="top">
  <div>
    <h1>Reports</h1>
    <div class="muted sub">Charts of your counts and waiting lists, to view here or save as a file.</div>
  </div>
</div>

<div class="card panel controls">
  <div class="row">
    <fieldset>
      <legend>Report on</legend>
      <label class="radio"><input type="radio" bind:group={kind} value="performance" /> My counts (targets and tallies)</label>
      <label class="radio"><input type="radio" bind:group={kind} value="waitingList" /> Waiting lists</label>
    </fieldset>
    <label
      >Each point is a
      <select bind:value={span} onchange={() => (from = defaultFrom(span))}>
        <option value="day">Day</option>
        <option value="week">Week</option>
        <option value="month">Month</option>
      </select>
    </label>
    <label>From <input type="date" bind:value={from} /></label>
    <label>To <input type="date" bind:value={to} /></label>
  </div>

  {#if kind === 'performance' && activityNames.length}
    <fieldset class="picks">
      <legend>Activities (none ticked = all)</legend>
      {#each activityNames as a (a)}
        <label class="radio"
          ><input type="checkbox" checked={activities.includes(a)} onchange={() => (activities = toggle(activities, a))} />
          {a}</label
        >
      {/each}
    </fieldset>
  {:else if kind === 'waitingList'}
    <fieldset class="picks">
      <legend>Lists (none ticked = every list not archived)</legend>
      {#each openLists as l (l.id)}
        <label class="radio"
          ><input type="checkbox" checked={listIds.includes(l.id)} onchange={() => (listIds = toggle(listIds, l.id))} />
          {l.name}{l.archived ? ' (archived)' : ''}</label
        >
      {:else}
        <span class="muted small">No waiting lists yet. Add them under Waiting lists.</span>
      {/each}
    </fieldset>
  {/if}

  <div class="btns">
    <button class="btn p" disabled={!!busy} onclick={show}>{busy === 'preview' ? 'Working…' : 'Show report'}</button>
    <span class="muted small">Save as:</span>
    {#each formats as [f, name] (f)}
      <button class="btn" disabled={!!busy} onclick={() => save(f, name)}>{busy === f ? 'Saving…' : name}</button>
    {/each}
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

{#if preview}
  {@const r = preview.report}
  <article class="report">
    <h2>{r.title}</h2>
    <p class="muted">{r.period} · made {r.made}</p>

    {#if preview.summary.length}
      <div class="scroll">
        <table>
          <thead>
            <tr>
              {#each ['Measure', 'Latest', 'Target', 'Variation', 'Assurance', 'Target met'] as h (h)}<th scope="col">{h}</th>{/each}
            </tr>
          </thead>
          <tbody>
            {#each preview.summary as row, i (i)}
              {@const s = r.summary[i]}
              <tr>
                {#each row as c, j (j)}
                  <td
                    class:improve={j === 3 && s.variation?.point === 'improvement'}
                    class:concern={(j === 3 && s.variation?.point === 'concern') || (j === 4 && s.assurance === 'fail')}
                    class:pass={j === 4 && s.assurance === 'pass'}>{c}</td
                  >
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}

    {#each r.sections as sec, i (i)}
      <section>
        <h3>{sec.title}</h3>
        {#if sec.intro}<p class="muted">{sec.intro}</p>{/if}
        {#each sec.charts as ch, j (j)}
          <figure>
            <figcaption>{ch.title}</figcaption>
            <img src={svgUrl(preview.charts[i][j])} alt={ch.alt} />
          </figure>
        {/each}
        {#if sec.table.rows.length}
          <details>
            <summary>Figures as a table</summary>
            <div class="scroll">
              <table>
                <thead>
                  <tr>{#each sec.table.columns as h, k (k)}<th scope="col">{h}</th>{/each}</tr>
                </thead>
                <tbody>
                  {#each sec.table.rows as row, k (k)}
                    <tr>{#each row as c, m (m)}<td class:num={typeof c !== 'string'}>{cell(c)}</td>{/each}</tr>
                  {/each}
                </tbody>
              </table>
            </div>
          </details>
        {/if}
      </section>
    {:else}
      <p class="muted">Nothing to show for this period. Add counts on the dashboard or figures under Waiting lists.</p>
    {/each}

    {#if r.footnote}<p class="muted small foot">{r.footnote}</p>{/if}
  </article>
{/if}

<details class="explain">
  <summary>How to read these charts</summary>
  <p>
    The charts follow the "Making Data Count" method used across health services. Each chart shows the figures with their
    average and process limits: the range ordinary ups and downs stay within. A point coloured blue is a real change for
    the better; orange is a real change for the worse; grey is ordinary variation. Limits need at least 15 points to be
    reliable, so with fewer they are marked "trial". The red line is your target, if you set one. Assurance says
    whether the target will be met every time, never, or only some of the time. A blank figure is "not entered" and shows
    as a gap, never as 0.
  </p>
</details>

<style>
  .sub {
    margin-top: 4px;
  }
  .panel {
    padding: 16px 18px;
    margin-top: 18px;
  }
  .row {
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
    align-items: end;
  }
  .row select,
  .row input {
    width: auto;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 13px;
  }
  label.radio {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }
  label.radio input {
    width: auto;
  }
  fieldset {
    border: 0;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  legend {
    font-size: 13px;
    color: var(--muted);
    margin-bottom: 4px;
  }
  .picks {
    margin-top: 12px;
    flex-direction: row;
    flex-wrap: wrap;
    gap: 6px 16px;
  }
  .picks legend {
    width: 100%;
  }
  .btns {
    display: flex;
    gap: 8px;
    margin-top: 14px;
    align-items: center;
    flex-wrap: wrap;
  }
  .small {
    font-size: 12px;
  }
  .error {
    color: var(--urgent);
    margin-top: 10px;
  }
  .report {
    margin-top: 20px;
    max-width: 1000px;
  }
  .report h2 {
    font-size: 20px;
  }
  .report section {
    margin-top: 24px;
  }
  .report h3 {
    font-size: 16px;
    margin-bottom: 6px;
  }
  figure {
    margin: 12px 0;
  }
  figcaption {
    font-weight: 600;
    font-size: 14px;
    margin-bottom: 6px;
  }
  figure img {
    width: 100%;
    height: auto;
    border-radius: 10px;
    border: 1px solid var(--line);
    background: #fff;
  }
  .scroll {
    overflow-x: auto;
    margin-top: 10px;
  }
  table {
    border-collapse: collapse;
    font-size: 13px;
    min-width: 100%;
  }
  th,
  td {
    border: 1px solid var(--line);
    padding: 5px 8px;
    text-align: left;
    vertical-align: top;
  }
  th {
    background: var(--panel2);
    font-weight: 600;
  }
  td.num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  td.improve {
    box-shadow: inset 4px 0 0 #00b0f0;
  }
  td.concern {
    box-shadow: inset 4px 0 0 #e46c0a;
  }
  td.pass {
    box-shadow: inset 4px 0 0 #00b0f0;
  }
  details summary {
    cursor: pointer;
    margin-top: 8px;
    font-size: 13px;
  }
  .foot {
    margin-top: 24px;
  }
</style>
