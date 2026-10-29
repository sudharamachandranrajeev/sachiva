<script lang="ts">
  import { onMount } from 'svelte'
  import { api, message, type RollbackPlan, type UpdateMode } from './api'
  import { app } from './state.svelte'
  import { kindName, updates } from './updates.svelte'

  let plan = $state<RollbackPlan | null>(null)
  let confirming = $state(false)
  let checking = $state(false)
  const s = $derived(updates.status)

  onMount(async () => {
    try {
      await updates.load()
      plan = await api.rollbackPlan()
    } catch (e) {
      app.say(message(e), { error: true })
    }
  })

  async function setMode(mode: UpdateMode) {
    try {
      await api.setUpdateMode(mode)
      await updates.load()
    } catch (e) {
      app.say(message(e), { error: true })
    }
  }

  async function checkNow() {
    checking = true
    updates.dismissed = false
    await updates.check()
    checking = false
  }
</script>

<section class="card">
  <h2>Updates</h2>
  {#if s}
    <p class="muted">
      This is Sachiva {s.current}, {kindName[s.kind]}.
      {#if s.canInstall}
        It can update itself. Every download is checked before it is installed.
      {:else if s.kind === 'portable'}
        The portable version cannot update itself: it tells you at start when a new version is out.
      {:else}
        It cannot update itself: it tells you at start when a new version is out.
      {/if}
    </p>
    <label class="field">
      At start
      <select value={s.mode} onchange={(e) => setMode(e.currentTarget.value as UpdateMode)}>
        <option value="ask">Check for updates and ask me before installing</option>
        <option value="off">Do not check</option>
      </select>
      <span class="hint">Checking needs the internet. Without it, Sachiva skips the check and says nothing.</span>
    </label>
    <button class="btn" disabled={checking} onclick={checkNow}>{checking ? 'Checking…' : 'Check now'}</button>
    {#if updates.found}
      <p class="hint">Sachiva {updates.found.version} is out: see the message at the top of the page.</p>
    {/if}

    <h3>Go back a version</h3>
    {#if s.kind === 'android'}
      <p class="hint">
        Android does not install an older version over a newer one. If an update causes trouble, report it below. Your
        data from before each update is kept under Backups and can be restored.
      </p>
    {:else if !plan}
      <p class="hint">There is no earlier version to go back to yet. This appears after Sachiva updates itself.</p>
    {:else if !s.canInstall}
      <p class="hint">This copy cannot install versions itself. Download {plan.version} from the release page.</p>
    {:else}
      <p class="hint">
        Before the last update you had Sachiva {plan.version}. Going back downloads that version, checks it the same way
        as an update, makes a backup of your data, and restarts.
      </p>
      {#if plan.needsRestore}
        <p class="hint warn">
          Version {plan.version} cannot read your data as it is now. Going back puts the backup from before the update in
          its place, so changes made since the update are lost. Your current data is backed up first.
        </p>
      {/if}
      {#if confirming}
        <div class="row">
          <button class="btn p" disabled={updates.busy} onclick={() => updates.rollback(plan!.needsRestore)}>
            {updates.busy ? 'Working…' : `Go back to ${plan.version}`}
          </button>
          <button class="btn" disabled={updates.busy} onclick={() => (confirming = false)}>Cancel</button>
        </div>
        {#if updates.progress !== null}<progress max="1" value={updates.progress} aria-label="Download"></progress>{/if}
      {:else}
        <button class="btn" onclick={() => (confirming = true)}>Go back to {plan.version}…</button>
      {/if}
    {/if}
  {/if}
</section>

<style>
  section {
    padding: 18px 20px;
    margin-top: 16px;
    max-width: 860px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    align-items: flex-start;
  }
  section h3 {
    margin-top: 8px;
  }
  .hint {
    font-size: 12px;
    font-weight: 400;
    color: var(--muted);
  }
  .warn {
    color: var(--warn);
    font-weight: 600;
  }
  .row {
    display: flex;
    gap: 8px;
  }
  progress {
    width: 100%;
    accent-color: var(--accent);
  }
</style>
