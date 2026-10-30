<script lang="ts">
  import { flip } from 'svelte/animate'
  import { api, message, type Item, type Lane, type Task, type Trashed } from './api'
  import { app } from './state.svelte'
  import { prefs } from './prefs.svelte'
  import { when } from './time'
  import TaskRow from './TaskRow.svelte'

  type Tab = 'open' | 'done' | 'trash'
  let tab = $state<Tab>('open')
  let search = $state('')
  let lane = $state<Lane | ''>('')
  let site = $state('')
  let tag = $state('')
  let done = $state<Task[]>([])
  let trash = $state<Trashed[]>([])
  let error = $state('')

  const laneNames: Record<Lane, string> = {
    overdue: 'Overdue',
    today: 'Today',
    waiting: 'Waiting on others',
    later: 'Later',
  }

  // Done and trash are fetched when shown, and again after every change.
  $effect(() => {
    void app.version
    error = ''
    if (tab === 'done') api.doneTasks().then((t) => (done = t), (e) => (error = message(e)))
    if (tab === 'trash') api.trash().then((t) => (trash = t), (e) => (error = message(e)))
  })

  const words = $derived(search.toLowerCase().split(/\s+/).filter(Boolean))
  function matches(t: Task): boolean {
    const hay = `${t.title} ${t.notes} ${t.site} ${t.tags.join(' ')}`.toLowerCase()
    return (
      words.every((w) => hay.includes(w)) &&
      (!site || t.site.trim() === site) &&
      (!tag || t.tags.includes(tag))
    )
  }

  const open = $derived.by(() => {
    const all: Item[] = app.dash ? [...(app.dash.now ? [app.dash.now] : []), ...app.dash.items] : []
    return all.filter((i) => (!lane || i.lane === lane) && matches(i.task))
  })
  // Targets and tallies have no score, so they sit below the scored jobs.
  const counters = $derived(lane ? [] : app.counters.filter((c) => matches(c.task)))
  const doneShown = $derived(done.filter(matches))
  const trashShown = $derived(trash.filter((t) => matches(t.task)))
  const filtered = $derived(words.length > 0 || !!lane || !!site || !!tag)
</script>

<div class="top">
  <div>
    <h1>Tasks</h1>
    <div class="muted sub">Every job, highest score first.</div>
  </div>
  <button class="btn p" onclick={() => app.startNew()}>+ New job</button>
</div>

<div class="tabs" role="tablist">
  {#each [['open', 'Open'], ['done', 'Done'], ['trash', 'Trash']] as [id, name] (id)}
    <button role="tab" aria-selected={tab === id} class:sel={tab === id} onclick={() => (tab = id as Tab)}>{name}</button>
  {/each}
</div>

<div class="filters">
  <input type="search" placeholder="Search titles, notes, sites, tags" bind:value={search} aria-label="Search" />
  {#if tab === 'open'}
    <select bind:value={lane} aria-label="Lane">
      <option value="">All lanes</option>
      {#each Object.entries(laneNames) as [id, name] (id)}<option value={id}>{name}</option>{/each}
    </select>
  {/if}
  {#if app.sites.length}
    <select bind:value={site} aria-label="Site">
      <option value="">All sites</option>
      {#each app.sites as s (s)}<option>{s}</option>{/each}
    </select>
  {/if}
  {#if app.tags.length}
    <select bind:value={tag} aria-label="Tag">
      <option value="">All tags</option>
      {#each app.tags as t (t)}<option>{t}</option>{/each}
    </select>
  {/if}
</div>

{#if error}<p class="error">{error}</p>{/if}

<div class="list">
  {#if tab === 'open'}
    {#each open as i (i.task.id)}
      <div animate:flip={{ duration: prefs.ms(250) }}>
        <TaskRow task={i.task} score={i.score} why={i.why} overdue={i.overdue} />
      </div>
    {:else}
      {#if !counters.length}
        <p class="muted">{filtered ? 'No open jobs match.' : 'No open jobs. Add one with + New job.'}</p>
      {/if}
    {/each}
    {#if counters.length}
      <h3 class="group">Targets and tallies</h3>
      {#each counters as c (c.task.id)}
        <TaskRow task={c.task} why={c.progress.line} />
      {/each}
    {/if}
  {:else if tab === 'done'}
    {#each doneShown as t (t.id)}
      <div class="row">
        <TaskRow task={t} />
        <button class="btn" onclick={() => app.act(() => api.reopen(t.id), `Back on the list: ${t.title}.`)}
          >Put back</button
        >
      </div>
    {:else}
      <p class="muted">{filtered ? 'No finished jobs match.' : 'Finished jobs appear here.'}</p>
    {/each}
  {:else}
    {#each trashShown as t (t.task.id)}
      <div class="row">
        <div class="card gone">
          <span>{t.task.title}</span>
          <span class="muted">deleted {when(t.deletedAt)}</span>
        </div>
        <button class="btn" onclick={() => app.act(() => api.restoreTask(t.task.id), `Restored: ${t.task.title}.`)}
          >Restore</button
        >
      </div>
    {:else}
      <p class="muted">{filtered ? 'Nothing in the trash matches.' : 'The trash is empty.'}</p>
    {/each}
  {/if}
</div>

<details class="explain">
  <summary>What is this page?</summary>
  <p>
    <b>Open</b> lists every job not yet done, in the same order as the dashboard: pinned first, then by score. Hover or
    open a job to see why it scored as it did. <b>Done</b> shows the last 200 finished jobs; <b>Put back</b> reopens one.
    <b>Trash</b> keeps deleted jobs for 30 days, then removes them for good. Search looks at titles, notes, sites and
    tags.
  </p>
</details>

<style>
  .top {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 16px;
  }
  .sub {
    margin-top: 4px;
  }
  .tabs {
    display: flex;
    gap: 4px;
    margin: 18px 0 12px;
    border-bottom: 1px solid var(--line);
  }
  .tabs button {
    padding: 8px 14px;
    color: var(--muted);
    font-weight: 600;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
  }
  .tabs button.sel {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }
  .filters {
    display: flex;
    gap: 8px;
    margin-bottom: 14px;
    flex-wrap: wrap;
  }
  .filters input {
    flex: 1 1 240px;
  }
  .filters select {
    width: auto;
    flex: 0 1 180px;
  }
  .list {
    max-width: 860px;
    margin-bottom: 18px;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 10px;
    align-items: start;
  }
  .gone {
    padding: 10px 14px;
    margin-bottom: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    border-radius: 14px;
  }
  .gone .muted {
    font-size: 12px;
  }
  .error {
    color: var(--urgent);
    margin-bottom: 10px;
  }
  .group {
    font-size: 13px;
    color: var(--muted);
    margin: 16px 4px 8px;
  }
</style>
