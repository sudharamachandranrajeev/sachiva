<script lang="ts">
  import { api, message, type Counter } from './api'
  import { app } from './state.svelte'

  let { counters }: { counters: Counter[] } = $props()

  /** The number typed for "+N", per job. */
  let amount = $state<Record<string, number>>({})
  let busy = $state<string | null>(null)

  const per = { day: 'today', week: 'this week', month: 'this month' } as const

  async function add(c: Counter, delta: number, note = '') {
    if (!Number.isInteger(delta) || delta === 0) return
    busy = c.task.id
    try {
      await api.addCount(c.task.id, delta, note)
      const unit = c.progress.unit.trim() || 'done'
      app.say(delta > 0 ? `Added ${delta} ${unit}: ${c.task.title}.` : `Took away ${-delta} ${unit}: ${c.task.title}.`)
      await app.refresh()
    } catch (e) {
      app.say(message(e), { error: true })
    } finally {
      busy = null
    }
  }

  /** Undo the last entry by adding its opposite, so the record stays complete. */
  async function undoLast(c: Counter) {
    try {
      const last = (await api.recentCounts(c.task.id)).find((x) => !x.note.startsWith('Undo'))
      if (!last) return app.say('Nothing to undo on this count.')
      await add(c, -last.delta, `Undo of ${last.delta > 0 ? '+' : ''}${last.delta}`)
    } catch (e) {
      app.say(message(e), { error: true })
    }
  }

  const pct = (c: Counter) =>
    c.progress.goal ? Math.min(100, Math.round((c.progress.done / c.progress.goal) * 100)) : 0
</script>

{#if counters.length}
  <section class="counters" aria-label="Targets and tallies">
    <h3>Targets and tallies</h3>
    <div class="cards">
      {#each counters as c (c.task.id)}
        {@const p = c.progress}
        <div class="card counter" class:behind={p.behind}>
          <div class="head">
            <button class="title" onclick={() => app.edit(c.task)}>{c.task.title}</button>
            {#if p.behind}<span class="flag">Behind</span>{/if}
          </div>
          <div class="big">
            <span class="n">{p.done}</span>
            {#if p.goal}<span class="muted">of {p.goal}</span>{/if}
            <span class="muted">{p.unit.trim() || 'done'} {p.span ? per[p.span] : p.goal ? 'so far' : 'in all'}</span>
          </div>
          {#if p.goal}
            <div
              class="bar"
              role="progressbar"
              aria-valuemin="0"
              aria-valuemax={p.goal}
              aria-valuenow={Math.min(p.done, p.goal)}
              aria-label="{c.task.title}: {p.done} of {p.goal}"
            >
              <span style="width: {pct(c)}%"></span>
              {#if p.expected !== null && p.expected < p.goal}
                <i style="left: {Math.round((p.expected / p.goal) * 100)}%" title="Even pace: {p.expected} by now"></i>
              {/if}
            </div>
          {/if}
          <div class="muted small">{p.line}</div>
          {#if p.met && p.met[1] > 0}
            <div class="muted small">Met in {p.met[0]} of the last {p.met[1]}.</div>
          {/if}
          <div class="btns">
            <button class="btn p" disabled={busy === c.task.id} onclick={() => add(c, 1)}>+1</button>
            <input
              type="number"
              min="1"
              max="10000"
              placeholder="N"
              aria-label="How many to add"
              bind:value={amount[c.task.id]}
              onkeydown={(e) => e.key === 'Enter' && add(c, Math.round(amount[c.task.id] ?? 0))}
            />
            <button
              class="btn"
              disabled={busy === c.task.id || !amount[c.task.id]}
              onclick={() => add(c, Math.round(amount[c.task.id] ?? 0))}>Add</button
            >
            <button class="btn" disabled={busy === c.task.id || p.done === 0} onclick={() => undoLast(c)}>Undo last</button>
          </div>
        </div>
      {/each}
    </div>
  </section>
{/if}

<style>
  .counters {
    margin-top: 18px;
  }
  h3 {
    font-size: 13px;
    color: var(--muted);
    font-weight: 600;
    margin: 0 4px 8px;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px;
  }
  .counter {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    border-left: 4px solid var(--accent);
  }
  .counter.behind {
    border-left-color: var(--warn);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .title {
    font-weight: 600;
    text-align: left;
    flex: 1;
    overflow-wrap: anywhere;
  }
  .title:hover {
    color: var(--accent);
  }
  .flag {
    font-size: 11px;
    font-weight: 700;
    color: var(--warn);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .big {
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex-wrap: wrap;
  }
  .big .n {
    font-size: 26px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .bar {
    position: relative;
    height: 8px;
    border-radius: 999px;
    background: var(--panel2);
    border: 1px solid var(--line);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
  }
  .behind .bar span {
    background: var(--warn);
  }
  .bar i {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--ink);
  }
  .small {
    font-size: 12px;
  }
  .btns {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 4px;
  }
  .btns input {
    width: 70px;
  }
</style>
