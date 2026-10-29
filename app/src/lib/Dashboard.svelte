<script lang="ts">
  import { flip } from 'svelte/animate'
  import { fade, fly } from 'svelte/transition'
  import { api, type AlarmAccess, type Lane } from './api'
  import { app } from './state.svelte'
  import { prefs } from './prefs.svelte'
  import { greeting, longDate } from './time'
  import ScoreRing from './ScoreRing.svelte'
  import TaskRow from './TaskRow.svelte'

  let { now, alarms }: { now: Date; alarms: AlarmAccess | null } = $props()

  const d = $derived(app.dash)
  let shown = $state<string | null>(null)

  const lanes: { lane: Lane; name: string; empty: string; explain: string }[] = [
    {
      lane: 'overdue',
      name: 'Overdue',
      empty: 'Nothing overdue.',
      explain: 'Past their deadline or appointment time and not done.',
    },
    {
      lane: 'today',
      name: 'Today',
      empty: 'Nothing else due today.',
      explain: 'Due later today, or pinned by you.',
    },
    {
      lane: 'waiting',
      name: 'Waiting on others',
      empty: 'Not waiting on anyone.',
      explain: 'Handed to someone else. They come back when it is time to chase.',
    },
  ]
  const of = (l: Lane) => d?.items.filter((i) => i.lane === l) ?? []
  const later = $derived(of('later'))
  const open = $derived((d?.items.length ?? 0) + (d?.now ? 1 : 0))
  const alarmProblem = $derived(alarms && (!alarms.exact || !alarms.batteryUnrestricted))
</script>

<div class="top">
  <div>
    <h1>{greeting(now)}</h1>
    <div class="muted sub">{longDate(now)} · {open} open {open === 1 ? 'job' : 'jobs'}</div>
  </div>
  {#if d}
    <div class="kpis">
      {#each d.figures as f (f.label)}
        <button
          class="card kpi {f.tone}"
          class:sel={shown === f.label}
          aria-expanded={shown === f.label}
          onclick={() => (shown = shown === f.label ? null : f.label)}
        >
          <span class="n">{f.value}</span>
          <span class="l">{f.label}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>
{#if shown && d}
  <p class="kpi-explain card" transition:fade={{ duration: prefs.ms(150) }}>
    <b>{shown}:</b>
    {d.figures.find((f) => f.label === shown)?.explain}
  </p>
{/if}

{#if alarmProblem}
  <div class="card banner">
    <b>Reminders may arrive late.</b>
    {#if !alarms?.exact}Sachiva is not allowed to set exact alarms.{/if}
    {#if !alarms?.batteryUnrestricted}Battery saving may hold reminders back.{/if}
    <button class="btn" onclick={() => (app.view = 'settings')}>Fix in Settings</button>
  </div>
{/if}

<div class="grid">
  <section>
    {#if d?.now}
      {@const n = d.now}
      {#key n.task.id}
        <div class="card now" class:calm={!n.overdue && n.score < 85} in:fly={{ y: 8, duration: prefs.ms(250) }}>
          <div class="tag">Do this now</div>
          <h2>{n.task.title}</h2>
          <div class="why"><b>Why first:</b> {n.why}</div>
          <div class="ring"><ScoreRing score={n.score} /></div>
          <div class="btns">
            <button class="btn p" onclick={() => app.act(() => api.complete(n.task.id), `Done: ${n.task.title}.`)}
              >Done</button
            >
            <button
              class="btn"
              title="Quiet its reminders for an hour"
              onclick={() => app.act(() => api.snooze(n.task.id, 60), 'Reminders quiet for an hour.')}>I'm on it</button
            >
            <button class="btn" onclick={() => app.edit(n.task)}>Open</button>
          </div>
        </div>
      {/key}
    {:else if d}
      <div class="card now calm empty">
        <div class="tag">All clear</div>
        <h2>Nothing needs you right now.</h2>
        <div class="why">Add a job with <b>Quick capture</b>, or press <kbd>N</kbd>.</div>
      </div>
    {/if}

    <div class="lanes">
      {#each lanes as l (l.lane)}
        {@const items = of(l.lane)}
        <div class="lane">
          <h3 title={l.explain}>{l.name}<span>{items.length}</span></h3>
          {#each items as i (i.task.id)}
            <div animate:flip={{ duration: prefs.ms(250) }} transition:fade={{ duration: prefs.ms(150) }}>
              <TaskRow task={i.task} score={i.score} overdue={i.overdue} />
            </div>
          {:else}
            <p class="muted none">{l.empty}</p>
          {/each}
        </div>
      {/each}
    </div>

    {#if later.length}
      <div class="lane later">
        <h3 title="Jobs with a deadline after today, or no deadline.">Later<span>{later.length}</span></h3>
        {#each later.slice(0, 8) as i (i.task.id)}
          <div animate:flip={{ duration: prefs.ms(250) }}>
            <TaskRow task={i.task} score={i.score} />
          </div>
        {/each}
        {#if later.length > 8}
          <button class="btn" onclick={() => (app.view = 'tasks')}>See all {later.length} in Tasks</button>
        {/if}
      </div>
    {/if}
  </section>

  <aside>
    <div class="card box">
      <h3>What is the score?</h3>
      <p class="muted">
        0 to 99. It rises as a deadline gets close compared with how long the job takes, and for jobs you marked
        important. Overdue jobs climb further. Jobs waiting on someone count for less until it is time to chase them.
        Pinned jobs come first, except that an urgent job always takes the "Do this now" card.
      </p>
    </div>
    <div class="card box">
      <h3>How reminders work</h3>
      <p class="muted">
        You get a heads-up before a job's deadline, leaving time for the work, then a reminder when it is due. By
        default, overdue jobs repeat every two hours in working hours, and urgent ones (score 85 or more) every ten
        minutes until you act. Nothing fires for ten minutes after you edit a job. Change this in Settings.
      </p>
    </div>
    <details class="explain">
      <summary>What is this page?</summary>
      <p>
        Your jobs in the order to do them. The top card is the one to do now. The numbers at the top count jobs; tap
        one to see how. The page updates by itself every 30 seconds and whenever a job changes, because scores move as
        time passes.
      </p>
    </details>
  </aside>
</div>

<style>
  .top {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }
  .sub {
    margin-top: 4px;
  }
  .kpis {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .kpi {
    padding: 9px 14px;
    min-width: 96px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    border-radius: 14px;
    transition: border-color 0.15s;
  }
  .kpi.sel,
  .kpi:hover {
    border-color: var(--accent);
  }
  .kpi .n {
    font-size: 22px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .kpi .l {
    font-size: 12px;
    color: var(--muted);
  }
  .kpi.bad .n {
    color: var(--urgent);
  }
  .kpi.warn .n {
    color: var(--warn);
  }
  .kpi-explain {
    margin-top: 10px;
    padding: 10px 14px;
    font-size: 13px;
    color: var(--muted);
  }
  .kpi-explain b {
    color: var(--ink);
  }
  .banner {
    margin-top: 14px;
    padding: 12px 16px;
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
    border-left: 4px solid var(--warn);
    font-size: 13px;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 300px;
    gap: 20px;
    margin-top: 18px;
    align-items: start;
  }
  .now {
    padding: 20px 22px;
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 6px 18px;
    border-left: 4px solid var(--urgent);
  }
  .now.calm {
    border-left-color: var(--accent);
  }
  .tag {
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--urgent);
    font-weight: 700;
  }
  .calm .tag {
    color: var(--accent);
  }
  .now h2 {
    grid-column: 1;
    overflow-wrap: anywhere;
  }
  .why {
    color: var(--muted);
    grid-column: 1;
  }
  .why b {
    color: var(--ink);
    font-weight: 600;
  }
  .ring {
    grid-row: 1 / span 4;
    grid-column: 2;
    align-self: center;
  }
  .btns {
    display: flex;
    gap: 8px;
    margin-top: 10px;
    grid-column: 1;
    flex-wrap: wrap;
  }
  kbd {
    font: inherit;
    font-weight: 600;
    padding: 0 5px;
    border: 1px solid var(--line);
    border-radius: 5px;
  }
  .lanes {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 14px;
    margin-top: 16px;
  }
  .lane h3 {
    font-size: 13px;
    color: var(--muted);
    font-weight: 600;
    margin: 0 4px 8px;
    display: flex;
    justify-content: space-between;
  }
  .none {
    font-size: 13px;
    margin: 0 4px;
  }
  .later {
    margin-top: 18px;
  }
  aside {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .box {
    padding: 16px 18px;
  }
  .box h3 {
    margin-bottom: 8px;
  }
  .box p {
    font-size: 13px;
    line-height: 1.5;
  }
  @media (max-width: 1100px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
  @media (max-width: 760px) {
    .lanes {
      grid-template-columns: 1fr;
    }
    .kpis {
      width: calc(100% + 32px);
      margin: 0 -16px;
      padding: 2px 16px 4px;
      flex-wrap: nowrap;
      overflow-x: auto;
      scrollbar-width: none;
    }
    .kpi {
      flex: none;
      min-width: 88px;
      padding: 8px 12px;
    }
    .kpi .l {
      white-space: nowrap;
    }
    .kpi .n {
      font-size: 19px;
    }
    .kpi .l {
      font-size: 11px;
    }
    .now {
      padding: 16px;
    }
    h1 {
      font-size: 22px;
    }
  }
</style>
