<script lang="ts">
  import { flip } from 'svelte/animate'
  import { fade, fly } from 'svelte/transition'
  import CircleAlert from '@lucide/svelte/icons/circle-alert'
  import CalendarDays from '@lucide/svelte/icons/calendar-days'
  import PhoneOutgoing from '@lucide/svelte/icons/phone-outgoing'
  import Hourglass from '@lucide/svelte/icons/hourglass'
  import CheckCheck from '@lucide/svelte/icons/check-check'
  import Sparkles from '@lucide/svelte/icons/sparkles'
  import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal'
  import Check from '@lucide/svelte/icons/check'
  import Timer from '@lucide/svelte/icons/timer'
  import FolderOpen from '@lucide/svelte/icons/folder-open'
  import ArrowRight from '@lucide/svelte/icons/arrow-right'
  import { api, type AlarmAccess, type Lane } from './api'
  import { app } from './state.svelte'
  import { prefs } from './prefs.svelte'
  import { greeting, longDate } from './time'
  import Counters from './Counters.svelte'
  import Focus from './Focus.svelte'
  import ClinicSummary from './ClinicSummary.svelte'
  import ScoreRing from './ScoreRing.svelte'
  import TaskRow from './TaskRow.svelte'
  import PrioritySelect from './PrioritySelect.svelte'

  let { now, alarms }: { now: Date; alarms: AlarmAccess | null } = $props()

  const figureIcons: Record<string, typeof CircleAlert> = { overdue: CircleAlert, 'due today': CalendarDays, 'to chase': PhoneOutgoing, 'waiting on others': Hourglass, 'done today': CheckCheck }
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
    <div class="eyebrow"><Sparkles size={14} aria-hidden="true" /> YOUR WORKSPACE</div>
    <h1>{greeting(now)}</h1>
    <div class="muted sub">{longDate(now)} · {open} open {open === 1 ? 'job' : 'jobs'}</div>
    <button class="priority-link" onclick={() => (app.managingPriorities = true)}><SlidersHorizontal size={14} aria-hidden="true" /> Manage priorities</button>
  </div>
  {#if d}
    <div class="kpis">
      {#each d.figures as f (f.label)}
        {@const FigureIcon = figureIcons[f.label] ?? CalendarDays}
        <button
          class="card kpi {f.tone}"
          class:sel={shown === f.label}
          aria-expanded={shown === f.label}
          onclick={() => (shown = shown === f.label ? null : f.label)}
        >
          <span class="kpi-head"><span class="kpi-icon"><FigureIcon size={16} strokeWidth={1.9} aria-hidden="true" /></span><span class="l">{f.label}</span></span>
          <span class="n">{f.value}</span>
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
          <div class="tag"><span class="tag-icon"><Sparkles size={15} aria-hidden="true" /></span> Do this now</div>
          <h2>{n.task.title}</h2>
          <div class="why"><b>Why first:</b> {n.why}</div>
          <div class="ring"><ScoreRing score={n.score} /></div>
          <div class="btns">
            <button class="btn p" onclick={() => app.act(() => api.complete(n.task.id), `Done: ${n.task.title}.`)}
              ><Check size={15} aria-hidden="true" /> Done</button
            >
            <button
              class="btn"
              title="Quiet its reminders for an hour"
              onclick={() => app.act(() => api.snooze(n.task.id, 60), 'Reminders quiet for an hour.')}><Timer size={15} aria-hidden="true" /> I'm on it</button
            >
            <button class="btn" onclick={() => app.edit(n.task)}><FolderOpen size={15} aria-hidden="true" /> Open</button>
            <PrioritySelect task={n.task} />
            {#if !app.focusSessions.some((s) => !s.endedAt)}<button class="btn" onclick={() => app.act(() => api.startFocus(n.task.id), 'Focus started.')}>Start focus</button>{/if}
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

    <Focus />
    <Counters counters={app.counters} />
    <ClinicSummary />

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
          <button class="btn" onclick={() => (app.view = 'tasks')}>See all {later.length} in Tasks <ArrowRight size={14} aria-hidden="true" /></button>
        {/if}
      </div>
    {/if}
  </section>

  <aside>
    <div class="card box">
      <h3>What is the score?</h3>
      <p class="muted">
        0 to 99. Within a priority level it rises as a deadline gets close compared with how long the job takes, and for jobs you marked
        important. Overdue jobs climb further. Jobs waiting on someone count for less until it is time to chase them.
        Your priority order decides which jobs come first. Within the same level, an urgent job can take the "Do this now" card ahead of a pin.
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
  .eyebrow { display: flex; align-items: center; gap: 6px; color: var(--accent); font-size: 10px; letter-spacing: .14em; font-weight: 750; margin-bottom: 6px; }
  .priority-link { display: inline-flex; align-items: center; gap: 6px; color: var(--accent); font-size: 12px; font-weight: 650; margin-top: 10px; padding: 4px 0; }
  .priority-link:hover { text-decoration: underline; }
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
    padding: 13px 14px;
    min-width: 112px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    border-radius: 16px;
    gap: 6px;
    transition: border-color .22s ease, transform .22s ease, box-shadow .22s ease;
  }
  .kpi.sel,
  .kpi:hover {
    border-color: var(--accent);
  }
  .kpi-head { display: flex; align-items: center; gap: 7px; }
  .kpi-icon { display: grid; place-items: center; width: 27px; height: 27px; border-radius: 9px; color: var(--accent); background: var(--accent-soft); }
  .kpi.bad .kpi-icon { color: var(--urgent); background: var(--urgent-soft); }
  .kpi.warn .kpi-icon { color: var(--warn); background: var(--warn-soft); }
  @media (hover: hover) { .kpi:hover { transform: translateY(-2px); } }
  .kpi .n {
    font-size: 24px;
    font-weight: 700;
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
    background-image: radial-gradient(ellipse at 100% 0%, var(--accent-soft), transparent 58%), linear-gradient(135deg, var(--panel), var(--panel2));
  }
  :global(:root[data-theme='glass']) .now { background-image: radial-gradient(ellipse at 100% 0%, var(--accent-soft), transparent 58%), linear-gradient(135deg, rgba(255,255,255,.8), rgba(224,236,255,.85)); }
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
  .tag { display: flex; align-items: center; gap: 8px; }
  .tag-icon { display: inline-grid; place-items: center; width: 29px; height: 29px; border-radius: 10px; background: var(--urgent-soft); }
  .calm .tag-icon { background: var(--accent-soft); }
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
  .btns :global(.btn) { display: inline-flex; align-items: center; justify-content: center; gap: 6px; }
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
      min-width: 118px;
      padding: 10px 12px;
    }
    .kpi .l {
      white-space: nowrap;
    }
    .kpi .n {
      font-size: 21px;
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
