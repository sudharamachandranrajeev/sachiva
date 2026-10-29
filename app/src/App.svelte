<script lang="ts">
  import { onMount } from 'svelte'
  import { fly } from 'svelte/transition'
  import { api, message, onChanged, type AlarmAccess } from './lib/api'
  import { app, type View } from './lib/state.svelte'
  import { prefs } from './lib/prefs.svelte'
  import Capture from './lib/Capture.svelte'
  import Dashboard from './lib/Dashboard.svelte'
  import Editor from './lib/Editor.svelte'
  import Lock from './lib/Lock.svelte'
  import Settings from './lib/Settings.svelte'
  import Setup from './lib/Setup.svelte'
  import UpdateBanner from './lib/UpdateBanner.svelte'
  import { updates } from './lib/updates.svelte'
  import Tasks from './lib/Tasks.svelte'

  let now = $state(new Date())
  let alarms = $state<AlarmAccess | null>(null)
  let failed = $state('')

  const nav: [View, string][] = [
    ['dashboard', 'Dashboard'],
    ['tasks', 'Tasks'],
    ['settings', 'Settings'],
  ]
  const counts = $derived<Record<View, string>>({
    dashboard: '',
    tasks: app.dash ? String(app.dash.items.length + (app.dash.now ? 1 : 0)) : '',
    settings: '',
  })

  async function checkAlarms() {
    if (app.status?.platform !== 'android') return
    alarms = await api.alarmAccess().catch(() => null)
  }

  $effect(() => prefs.apply())

  // Check for an update once per run, when the app is open and set up.
  let checkedUpdates = false
  $effect(() => {
    const st = app.status
    if (checkedUpdates || !st?.unlocked || st.recoveryPending || !st.setupDone) return
    checkedUpdates = true
    updates.startup()
  })

  onMount(() => {
    let unlisten: (() => void) | undefined
    ;(async () => {
      try {
        await app.loadStatus()
        if (app.status?.notice) app.say(app.status.notice, { error: true })
        await app.refresh()
        await checkAlarms()
        unlisten = await onChanged(() => app.refresh())
      } catch (e) {
        failed = message(e)
      }
    })()
    // Scores move with the clock: refresh every 30 seconds and when the window comes back.
    const tick = setInterval(() => {
      now = new Date()
      app.refresh()
    }, 30_000)
    const back = () => {
      if (document.visibilityState === 'visible') {
        now = new Date()
        app.refresh()
        checkAlarms()
      }
    }
    document.addEventListener('visibilitychange', back)
    return () => {
      unlisten?.()
      clearInterval(tick)
      document.removeEventListener('visibilitychange', back)
    }
  })

  function typing(e: KeyboardEvent): boolean {
    const el = e.target as HTMLElement
    return el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName)
  }

  function keys(e: KeyboardEvent) {
    if (!app.status?.unlocked || app.status.recoveryPending || app.editing || app.capturing || typing(e)) return
    if ((e.ctrlKey || e.metaKey) && e.key === 'z') {
      e.preventDefault()
      app.undo()
    } else if (!e.ctrlKey && !e.metaKey && !e.altKey && e.key.toLowerCase() === 'n') {
      e.preventDefault()
      app.capturing = true
    }
  }
</script>

<svelte:window onkeydown={keys} />

{#if failed}
  <div class="fatal card">
    <h1>Sachiva could not start</h1>
    <p>{failed}</p>
  </div>
{:else if !app.status}
  <div class="loading muted">Opening…</div>
{:else if !app.status.unlocked}
  <Lock mode="locked" />
{:else if app.status.recoveryPending}
  <Lock mode="firstRun" />
{:else if !app.status.setupDone}
  <Setup {alarms} onAlarms={checkAlarms} />
{:else}
  <div class="shell">
    <nav class="rail" aria-label="Main">
      <div class="brand"><img src="/favicon.svg" alt="" width="26" height="26" />Sachiva</div>
      {#each nav as [id, name] (id)}
        <button class="nav" class:sel={app.view === id} aria-current={app.view === id ? 'page' : undefined} onclick={() => (app.view = id)}>
          {name}<b>{counts[id]}</b>
        </button>
      {/each}
      <div class="sp"></div>
      <button class="capture" onclick={() => (app.capturing = true)} title="Quick capture (N)">+ Quick capture</button>
      <button class="undo" onclick={() => app.undo()} title="Undo the last change (Ctrl+Z)">Undo last change</button>
    </nav>

    <main>
      <UpdateBanner />
      {#key app.view}
        <div in:fly={{ y: 6, duration: prefs.ms(200) }}>
          {#if app.view === 'dashboard'}
            <Dashboard {now} {alarms} />
          {:else if app.view === 'tasks'}
            <Tasks />
          {:else}
            <Settings {alarms} onAlarms={checkAlarms} />
          {/if}
        </div>
      {/key}
    </main>

    <nav class="dock card" aria-label="Main">
      {#each nav as [id, name] (id)}
        <button class:sel={app.view === id} aria-current={app.view === id ? 'page' : undefined} onclick={() => (app.view = id)}>{name}</button>
      {/each}
      <button class="fab" aria-label="Quick capture" onclick={() => (app.capturing = true)}>+</button>
    </nav>
  </div>

  {#if app.editing}
    {#key app.editing.task.id}
      <Editor task={app.editing.task} isNew={app.editing.isNew} />
    {/key}
  {/if}
  {#if app.capturing}
    <Capture />
  {/if}
{/if}

{#if app.toast}
  <div class="toast" class:error={app.toast.error} role="status" transition:fly={{ y: 12, duration: prefs.ms(180) }}>
    <span>{app.toast.text}</span>
    {#if app.toast.undo}<button onclick={() => app.undo()}>Undo</button>{/if}
    <button aria-label="Dismiss" onclick={() => (app.toast = null)}>×</button>
  </div>
{/if}

<style>
  .shell {
    display: flex;
    min-height: 100vh;
  }
  .rail {
    width: 220px;
    flex: none;
    padding: 22px 16px;
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 4px;
    background: var(--panel2);
    position: sticky;
    top: 0;
    height: 100vh;
  }
  .brand {
    font-weight: 600;
    font-size: 20px;
    padding: 0 10px 18px;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .nav {
    padding: 9px 10px;
    border-radius: 10px;
    color: var(--muted);
    font-size: 14px;
    display: flex;
    justify-content: space-between;
    text-align: left;
    transition: background-color 0.15s;
  }
  .nav:hover {
    background: var(--panel);
  }
  .nav.sel {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .nav b {
    font-weight: 500;
    font-size: 12px;
  }
  .sp {
    flex: 1;
  }
  .capture {
    margin: 0 4px;
    padding: 11px;
    border-radius: 12px;
    background: var(--accent);
    color: var(--on-accent);
    font-weight: 600;
  }
  .undo {
    margin-top: 6px;
    padding: 8px;
    color: var(--muted);
    font-size: 13px;
  }
  main {
    flex: 1;
    min-width: 0;
    padding: 24px 28px 40px;
  }
  .dock {
    display: none;
  }
  .toast {
    position: fixed;
    left: 50%;
    bottom: 24px;
    transform: translateX(-50%);
    background: var(--ink);
    color: var(--bg);
    padding: 10px 12px 10px 16px;
    border-radius: 12px;
    display: flex;
    gap: 12px;
    align-items: center;
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.25);
    z-index: 10;
    max-width: calc(100vw - 32px);
  }
  .toast.error {
    background: var(--urgent);
    color: #fff;
  }
  .toast button {
    font-weight: 700;
    color: inherit;
    text-decoration: underline;
  }
  .loading,
  .fatal {
    margin: 20vh auto;
    width: fit-content;
    padding: 24px;
  }
  .fatal {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  @media (max-width: 760px) {
    .rail {
      display: none;
    }
    main {
      padding: 18px 16px 110px;
    }
    .dock {
      display: flex;
      position: fixed;
      left: 12px;
      right: 12px;
      bottom: calc(12px + env(safe-area-inset-bottom));
      padding: 8px;
      justify-content: space-around;
      align-items: center;
      border-radius: 22px;
      z-index: 5;
    }
    .dock button {
      font-size: 13px;
      color: var(--muted);
      padding: 8px 12px;
      border-radius: 14px;
    }
    .dock button.sel {
      color: var(--accent);
      font-weight: 700;
      background: var(--accent-soft);
    }
    .dock .fab {
      width: 46px;
      height: 46px;
      border-radius: 50%;
      background: var(--accent);
      color: var(--on-accent);
      font-size: 26px;
      font-weight: 300;
      padding: 0;
    }
    .toast {
      bottom: calc(90px + env(safe-area-inset-bottom));
    }
  }
</style>
