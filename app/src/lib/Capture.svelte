<script lang="ts">
  import { onMount } from 'svelte'
  import { api, message, type Finding } from './api'
  import { app } from './state.svelte'

  // Quick capture: type a title, press Enter. Details can follow later.
  let title = $state('')
  let findings = $state<Finding[]>([])
  let dialog: HTMLDialogElement

  onMount(() => dialog.showModal())

  $effect(() => {
    const text = title
    const h = setTimeout(async () => {
      findings = text.trim() ? await api.check(text).catch(() => []) : []
    }, 250)
    return () => clearTimeout(h)
  })

  async function add(details: boolean) {
    const text = title.trim()
    if (!text) return
    try {
      const saved = await api.saveTask(await api.newTask(text))
      app.capturing = false
      if (details) app.edit(saved)
      else app.say(`Added: ${saved.title}.`, { undo: true })
    } catch (e) {
      app.say(message(e), { error: true })
    }
  }
</script>

<dialog
  bind:this={dialog}
  aria-label="Quick capture"
  oncancel={(e) => {
    e.preventDefault()
    app.capturing = false
  }}
>
  <!-- svelte-ignore a11y_autofocus -->
  <input
    bind:value={title}
    autofocus
    maxlength="300"
    placeholder="What needs doing? Press Enter to add."
    onkeydown={(e) => {
      if (e.key === 'Enter') {
        e.preventDefault()
        add(e.shiftKey)
      }
    }}
  />
  {#if findings.length}
    <div class="check" role="status">
      {#each findings as f, i (i)}<div>“{f.text}”. {f.message}</div>{/each}
    </div>
  {/if}
  <div class="row">
    <span class="muted">Enter adds it. Shift + Enter adds it and opens the details.</span>
    <button class="btn" onclick={() => add(true)} disabled={!title.trim()}>Add with details</button>
    <button class="btn p" onclick={() => add(false)} disabled={!title.trim()}>Add</button>
  </div>
</dialog>

<style>
  dialog {
    margin: 12vh auto auto;
    width: min(640px, calc(100vw - 32px));
    padding: 14px;
    border: 1px solid var(--line);
    border-radius: var(--r);
    background: var(--panel);
    color: var(--ink);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.25);
    animation: rise 0.18s ease-out;
  }
  dialog::backdrop {
    background: rgba(10, 25, 22, 0.35);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }
  }
  input {
    font-size: 17px;
    padding: 12px 14px;
  }
  .check {
    margin-top: 10px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--warn-soft);
    border-left: 4px solid var(--warn);
    font-size: 13px;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 10px;
    flex-wrap: wrap;
  }
  .row .muted {
    flex: 1;
    font-size: 12px;
  }
</style>
