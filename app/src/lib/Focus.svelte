<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from './api'
  import { app } from './state.svelte'
  import { when } from './time'

  let clock = $state(Date.now())
  const active = $derived(app.focusSessions.find((s) => !s.endedAt))
  const recent = $derived(app.focusSessions.filter((s) => s.endedAt))
  onMount(() => {
    const timer = setInterval(() => (clock = Date.now()), 1000)
    return () => clearInterval(timer)
  })
  function duration(start: string, end: number): string {
    const seconds = Math.max(0, Math.floor((end - new Date(start).getTime()) / 1000))
    const hours = Math.floor(seconds / 3600)
    const mins = Math.floor((seconds % 3600) / 60)
    const secs = seconds % 60
    return `${hours ? `${hours} h ` : ''}${mins} min ${String(secs).padStart(2, '0')} s`
  }
</script>

<section class="card focus" aria-label="Focus timer and time log">
  <h3>Focus</h3>
  {#if active}
    <p>Working on <b>{active.title}</b></p>
    <p class="elapsed">{duration(active.startedAt, clock)}</p>
    <button class="btn" onclick={() => app.act(() => api.stopFocus(active.id), 'Focus session stopped.')}>Stop and log time</button>
    <p class="muted hint">The timer continues until you stop it, even if Sachiva closes.</p>
  {:else}
    <p class="muted">Start a timer on a job to record your time.</p>
  {/if}
  {#if recent.length}
    <details>
      <summary>Time log ({recent.length})</summary>
      <ul>{#each recent as session (session.id)}
        <li><b>{session.title}</b> · {duration(session.startedAt, new Date(session.endedAt!).getTime())} · {when(session.startedAt)}</li>
      {/each}</ul>
    </details>
  {/if}
</section>

<style>
  .focus { padding: 16px 18px; margin-top: 14px; }
  .focus h3 { margin-bottom: 8px; }
  .focus p { margin: 6px 0; }
  .elapsed { font-size: 22px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .hint, li { font-size: 12px; }
  details { margin-top: 10px; }
  ul { padding-left: 18px; }
</style>
