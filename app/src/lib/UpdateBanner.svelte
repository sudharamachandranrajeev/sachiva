<script lang="ts">
  import { fade } from 'svelte/transition'
  import { api } from './api'
  import { app } from './state.svelte'
  import { prefs } from './prefs.svelte'
  import { updates } from './updates.svelte'

  const u = $derived(updates.found)
  let notes = $state(false)
</script>

{#if u && !updates.dismissed}
  <div class="card banner" role="status" transition:fade={{ duration: prefs.ms(150) }}>
    <div class="text">
      <b>Sachiva {u.version} is out.</b>
      {#if u.canInstall}
        You have {updates.status?.current}. A backup is made before anything is installed, and you can go back
        afterwards in Settings.
      {:else if updates.status?.kind === 'portable'}
        This is the portable version, so it cannot update itself. Download the new version and replace this folder's
        files with it. Your data stays where it is.
      {:else}
        This copy cannot update itself. Download the new version from the release page.
      {/if}
      {#if u.notes}
        <button class="link" onclick={() => (notes = !notes)}>{notes ? 'Hide' : 'What is new'}</button>
      {/if}
    </div>
    {#if notes && u.notes}<pre class="notes">{u.notes}</pre>{/if}
    {#if updates.progress !== null}
      <progress max="1" value={updates.progress} aria-label="Download"></progress>
    {/if}
    <div class="btns">
      {#if u.canInstall}
        <button class="btn p" disabled={updates.busy} onclick={() => updates.install()}>
          {updates.busy ? 'Installing…' : 'Install and restart'}
        </button>
      {:else}
        <button class="btn p" onclick={() => api.openPage('releases').catch((e) => app.say(String(e), { error: true }))}>
          Open the download page
        </button>
      {/if}
      <button class="btn" disabled={updates.busy} onclick={() => (updates.dismissed = true)}>Later</button>
    </div>
  </div>
{/if}

<style>
  .banner {
    margin-bottom: 14px;
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    border-left: 4px solid var(--accent);
    font-size: 13px;
  }
  .link {
    background: none;
    border: 0;
    padding: 0;
    color: var(--accent);
    text-decoration: underline;
    font: inherit;
    cursor: pointer;
  }
  .notes {
    white-space: pre-wrap;
    font: inherit;
    max-height: 200px;
    overflow: auto;
    color: var(--muted);
  }
  progress {
    width: 100%;
    accent-color: var(--accent);
  }
  .btns {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
</style>
