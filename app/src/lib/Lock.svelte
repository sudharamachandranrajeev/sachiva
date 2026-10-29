<script lang="ts">
  import { onMount } from 'svelte'
  import { api, message } from './api'
  import { app } from './state.svelte'

  // Two screens: unlock with the recovery key when this device's key is
  // missing or wrong, or, on first run, show the key to write down.
  let { mode }: { mode: 'locked' | 'firstRun' } = $props()

  let key = $state('')
  let entered = $state('')
  let wrote = $state(false)
  let error = $state('')
  let busy = $state(false)

  onMount(async () => {
    if (mode === 'firstRun') {
      try {
        key = await api.recoveryKey()
      } catch (e) {
        error = message(e)
      }
    }
  })

  async function unlock() {
    busy = true
    error = ''
    try {
      await api.unlockWith(entered)
      await app.loadStatus()
      await app.refresh()
    } catch (e) {
      error = message(e)
    } finally {
      busy = false
    }
  }

  async function confirm() {
    try {
      await api.recoverySaved()
      await app.loadStatus()
      await app.refresh()
    } catch (e) {
      error = message(e)
    }
  }
</script>

<div class="wrap">
  <div class="card panel">
    <div class="brand"><img src="/favicon.svg" alt="" width="36" height="36" /> Sachiva</div>
    {#if mode === 'firstRun'}
      <h1>Your recovery key</h1>
      <p>
        Sachiva keeps your jobs in an encrypted database on this device. This key opens it. Sachiva also keeps a copy in
        this device's secure store, so you will not need to type it day to day.
      </p>
      <p>
        <b>Write it down on paper and keep it somewhere safe.</b> If this device's secure store is reset or you move to a
        new device, the key is the only way to open your data and your backups. Nobody else has a copy, and it cannot be
        reset.
      </p>
      {#if key}<code class="key">{key}</code>{/if}
      <label class="check-row">
        <input type="checkbox" bind:checked={wrote} />
        I have written the key down
      </label>
      <button class="btn p" disabled={!wrote || !key} onclick={confirm}>Continue</button>
      <p class="muted small">You can see the key again later in Settings, under Your data.</p>
    {:else}
      <h1>Sachiva is locked</h1>
      <p>{app.status?.problem ?? 'The key for your data could not be read on this device.'}</p>
      <p>
        Enter the recovery key you wrote down when you first set up Sachiva. Your data has not been changed or removed.
      </p>
      <label class="field">
        Recovery key
        <input
          bind:value={entered}
          placeholder="xxxx-xxxx-xxxx-…"
          autocomplete="off"
          spellcheck="false"
          onkeydown={(e) => e.key === 'Enter' && unlock()}
        />
      </label>
      <button class="btn p" disabled={busy || !entered.trim()} onclick={unlock}>Unlock</button>
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
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
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 18px;
    font-weight: 600;
  }
  .key {
    font-size: 16px;
    letter-spacing: 0.04em;
    padding: 14px;
    border-radius: 12px;
    background: var(--panel2);
    border: 1px solid var(--line);
    overflow-wrap: anywhere;
    user-select: all;
  }
  .check-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .btn {
    align-self: flex-start;
  }
  .small {
    font-size: 12px;
  }
  .error {
    color: var(--urgent);
    font-weight: 600;
  }
</style>
