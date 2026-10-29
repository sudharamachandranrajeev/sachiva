<script lang="ts">
  import type { Task } from './api'
  import { app } from './state.svelte'
  import { api } from './api'
  import { when, minutes } from './time'

  let {
    task,
    score = null,
    why = '',
    overdue = false,
  }: { task: Task; score?: number | null; why?: string; overdue?: boolean } = $props()

  const waiting = $derived(task.status.state === 'waiting')
  const done = $derived(task.status.state === 'done')
  const kind = $derived(
    overdue || (score ?? 0) >= 85 ? 'u' : waiting ? 'w' : task.pinned || (score ?? 0) >= 60 ? 'a' : '',
  )
  const meta = $derived.by(() => {
    const m: string[] = []
    if (task.site) m.push(task.site)
    if (task.status.state === 'waiting') {
      m.push(task.status.on ? `waiting on ${task.status.on}` : 'waiting')
      if (task.status.chase) m.push(`chase ${when(task.status.chase)}`)
    } else if (task.status.state === 'done') {
      m.push(`done ${when(task.status.at)}`)
    } else {
      if (task.appointment) m.push(`appointment ${when(task.appointment)}`)
      if (task.due && task.due !== task.appointment) m.push(`due ${when(task.due)}`)
      if (task.effortMin) m.push(minutes(task.effortMin))
    }
    if (task.checklist.length) m.push(`checklist ${task.checklist.filter((c) => c.done).length} / ${task.checklist.length}`)
    if (task.repeat) m.push('repeats')
    if (task.snoozedUntil && new Date(task.snoozedUntil) > new Date()) m.push(`quiet until ${when(task.snoozedUntil)}`)
    return m.join(' · ')
  })
</script>

<div class="card task {kind}">
  {#if !done}
    <button
      class="tick"
      title="Mark done"
      aria-label="Mark “{task.title}” done"
      onclick={() => app.act(() => api.complete(task.id), `Done: ${task.title}.`)}
    ></button>
  {:else}
    <i class="dot"></i>
  {/if}
  <button class="body" onclick={() => app.edit(task)} title={why || undefined}>
    <span class="t">{#if task.pinned}<span class="pin" aria-label="pinned">●</span>{/if}{task.title}</span>
    {#if meta}<span class="m">{meta}</span>{/if}
    {#if why}<span class="m why">{why}</span>{/if}
  </button>
  {#if score != null}
    <span class="s" title="Priority score, 0 to 99. Higher means do it sooner.">{score}</span>
  {/if}
</div>

<style>
  .task {
    padding: 10px 12px;
    margin-bottom: 8px;
    display: grid;
    grid-template-columns: 20px 1fr auto;
    gap: 10px;
    align-items: start;
    border-radius: 14px;
    transition: border-color 0.15s, transform 0.15s;
  }
  .task:hover {
    border-color: var(--accent);
  }
  .tick {
    width: 18px;
    height: 18px;
    margin-top: 1px;
    border-radius: 50%;
    border: 2px solid var(--muted);
    transition: background-color 0.15s, border-color 0.15s;
  }
  .tick:hover {
    border-color: var(--ok);
    background: var(--accent-soft);
  }
  .u .tick {
    border-color: var(--urgent);
  }
  .w .tick {
    border-color: var(--warn);
  }
  .a .tick {
    border-color: var(--accent);
  }
  .dot {
    width: 8px;
    height: 8px;
    margin: 6px 5px;
    border-radius: 50%;
    background: var(--ok);
  }
  .body {
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .t {
    font-size: 14px;
    font-weight: 500;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  .pin {
    color: var(--accent);
    font-size: 9px;
    margin-right: 6px;
    vertical-align: 2px;
  }
  .m {
    font-size: 12px;
    color: var(--muted);
  }
  .why {
    font-style: italic;
  }
  .s {
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .u .s {
    color: var(--urgent);
  }
</style>
