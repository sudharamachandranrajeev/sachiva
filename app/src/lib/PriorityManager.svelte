<script lang="ts">
  import { onMount } from 'svelte'
  import { api, message, type Finding, type PriorityConfig } from './api'
  import { app } from './state.svelte'
  let dialog: HTMLDialogElement
  let config = $state<PriorityConfig>($state.snapshot(app.priorityConfig))
  let name = $state('')
  let busy = $state(false)
  let findings = $state<Finding[]>([])
  onMount(() => dialog.showModal())
  $effect(() => {
    const text = name
    let active = true
    const timer = setTimeout(async () => {
      try { const result = text.trim() ? await api.check(text) : []; if (active) findings = result }
      catch { if (active) findings = [] }
    }, 250)
    return () => { active = false; clearTimeout(timer) }
  })
  function close() { app.managingPriorities = false }
  function add() {
    const label = name.trim()
    if (!label) return
    if (config.levels.some((p) => p.label.toLowerCase() === label.toLowerCase())) {
      app.say('A priority already has that name.', { error: true }); return
    }
    if (config.levels.length >= 30) { app.say('You can have up to 30 priorities.', { error: true }); return }
    config.levels.push({ id: crypto.randomUUID(), label })
    name = ''
  }
  function move(index: number, direction: number) {
    const next = index + direction
    if (next < 0 || next >= config.levels.length) return
    const levels = [...config.levels]
    ;[levels[index], levels[next]] = [levels[next], levels[index]]
    config.levels = levels
  }
  async function save() {
    if (busy) return
    busy = true
    try {
      await api.savePriorityLevels($state.snapshot(config))
      await app.refresh()
      close()
      app.say('Priority order saved.', { undo: true })
    } catch (e) { app.say(message(e), { error: true }) }
    finally { busy = false }
  }
</script>
<dialog bind:this={dialog} aria-labelledby="priority-title" oncancel={(e) => { e.preventDefault(); close() }}>
  <h2 id="priority-title">Priority levels</h2>
  <p class="muted">Highest is at the top. Move levels to change the order of all jobs.</p>
  <ol>
    {#each [...config.levels].reverse() as p (p.id)}
      {@const index = config.levels.findIndex((item) => item.id === p.id)}
      <li><span>{p.label}</span>
        <button class="btn" aria-label="Move {p.label} higher" disabled={index === config.levels.length - 1} onclick={() => move(index, 1)}>↑</button>
        <button class="btn" aria-label="Move {p.label} lower" disabled={index === 0} onclick={() => move(index, -1)}>↓</button>
      </li>
    {/each}
  </ol>
  <label for="new-priority">New priority name</label>
  <div class="row"><input id="new-priority" bind:value={name} maxlength="60" placeholder="e.g. Critical" /><button class="btn" onclick={add} disabled={!name.trim()}>Add</button></div>
  {#if findings.length}<div class="check" role="status">{#each findings as f, i (i)}<div>“{f.text}”. {f.message}</div>{/each}</div>{/if}
  <div class="row actions"><button class="btn" onclick={close}>Cancel</button><button class="btn p" onclick={save} disabled={busy}>Save order</button></div>
</dialog>
<style>
  dialog { margin: auto; width: min(540px, calc(100vw - 32px)); max-height: 85vh; overflow-y: auto; padding: 20px; border: 1px solid var(--line); border-radius: var(--r); background: var(--panel); color: var(--ink); }
  dialog::backdrop { background: rgba(10, 25, 22, 0.4); }
  ol { padding: 0; list-style: none; }
  li { display: flex; gap: 8px; align-items: center; margin: 6px 0; }
  li span { flex: 1; overflow-wrap: anywhere; }
  .row { display: flex; gap: 8px; margin-top: 8px; }
  .row input { flex: 1; min-width: 0; }
  .check { background: var(--warn-soft); padding: 10px; margin-top: 8px; }
  .actions { justify-content: end; margin-top: 16px; }
</style>
