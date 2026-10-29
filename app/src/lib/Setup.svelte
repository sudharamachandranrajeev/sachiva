<script lang="ts">
  import { onMount } from 'svelte'
  import { api, message, type AlarmAccess, type UpdateMode } from './api'
  import { app } from './state.svelte'
  import { prefs, type Motion, type Theme } from './prefs.svelte'

  // First-run steps after the recovery key. Every step can be skipped, and
  // everything here can be changed later in Settings.
  let { alarms, onAlarms }: { alarms: AlarmAccess | null; onAlarms: () => void } = $props()

  const android = $derived(app.status?.platform === 'android')
  const steps = ['reminders', 'updates', 'look'] as const
  let step = $state(0)
  let autostart = $state(true)
  let mode = $state<UpdateMode>('ask')
  let error = $state('')

  onMount(async () => {
    if (!android) autostart = await api.autostart().catch(() => true)
  })

  async function next() {
    error = ''
    try {
      if (steps[step] === 'reminders' && !android) await api.setAutostart(autostart)
      if (steps[step] === 'updates') await api.setUpdateMode(mode)
      if (step < steps.length - 1) step++
      else await finish()
    } catch (e) {
      error = message(e)
    }
  }

  async function finish() {
    try {
      await api.setupDone()
      await app.loadStatus()
      await app.refresh()
    } catch (e) {
      error = message(e)
    }
  }
</script>

<div class="wrap">
  <div class="card panel">
    <div class="top">
      <div class="brand"><img src="/favicon.svg" alt="" width="30" height="30" /> Sachiva</div>
      <span class="muted small">Step {step + 1} of {steps.length}</span>
    </div>

    {#if steps[step] === 'reminders'}
      <h1>Reminders</h1>
      {#if android}
        <p>Android can hold reminders back to save battery. For reminders on time, allow both of these.</p>
        <div class="status">
          <span class="pill {alarms?.exact ? '' : 'bad'}">{alarms?.exact ? 'Allowed' : 'Not allowed'}</span>
          <span>Exact alarms</span>
          {#if !alarms?.exact}<button class="btn" onclick={() => api.openAlarmSettings().finally(onAlarms)}>Allow</button>{/if}
        </div>
        <div class="status">
          <span class="pill {alarms?.batteryUnrestricted ? '' : 'warn'}"
            >{alarms?.batteryUnrestricted ? 'Unrestricted' : 'Restricted'}</span
          >
          <span>Battery use</span>
          {#if !alarms?.batteryUnrestricted}
            <button class="btn" onclick={() => api.openBatterySettings().finally(onAlarms)}>Change</button>
          {/if}
        </div>
      {:else}
        <p>
          Reminders only come while Sachiva runs. Closing the window keeps it in the tray. Starting it with the computer
          means you never miss one.
        </p>
        <label class="check-row">
          <input type="checkbox" bind:checked={autostart} />
          Start Sachiva with the computer
        </label>
      {/if}
    {:else if steps[step] === 'updates'}
      <h1>Updates</h1>
      <p>
        Sachiva can check for a new version when it starts. It always asks before installing, backs up your data first,
        and lets you go back. Checking needs the internet and sends nothing about you or your jobs.
      </p>
      <label class="check-row"><input type="radio" bind:group={mode} value="ask" /> Check at start and ask me</label>
      <label class="check-row"><input type="radio" bind:group={mode} value="off" /> Do not check</label>
    {:else}
      <h1>Look</h1>
      <div class="grid">
        <label class="field">
          Colours
          <select bind:value={prefs.theme} onchange={() => prefs.apply()}>
            {#each [['system', 'Follow the system'], ['light', 'Light'], ['dark', 'Dark']] as [v, name] (v)}
              <option value={v as Theme}>{name}</option>
            {/each}
          </select>
        </label>
        <label class="field">
          Movement
          <select bind:value={prefs.motion} onchange={() => prefs.apply()}>
            {#each [['system', 'Follow the system'], ['reduced', 'Calm: as little as possible'], ['full', 'Full']] as [v, name] (v)}
              <option value={v as Motion}>{name}</option>
            {/each}
          </select>
        </label>
      </div>
    {/if}

    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="row">
      <button class="btn p" onclick={next}>{step < steps.length - 1 ? 'Next' : 'Start using Sachiva'}</button>
      {#if step > 0}<button class="btn" onclick={() => step--}>Back</button>{/if}
      <button class="btn ghost" onclick={finish}>Skip the rest</button>
    </div>
    <p class="muted small">You can change all of this later in Settings.</p>
  </div>
</div>

<style>
  .wrap {
    min-height: 100vh;
    display: grid;
    place-items: center;
    padding: 24px 16px;
  }
  .panel {
    width: min(560px, 100%);
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    line-height: 1.5;
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 17px;
    font-weight: 600;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }
  .status,
  .check-row,
  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
  }
  .ghost {
    background: none;
    border-color: transparent;
  }
  .small {
    font-size: 12px;
  }
  .error {
    color: var(--urgent);
    font-weight: 600;
  }
  @media (max-width: 760px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
