<script lang="ts">
  import { api, message, type Task, type WaitList } from './api'
  import { fromInput, toInput } from './time'
  let { task = $bindable(), stored }: { task: Task; stored: boolean } = $props()
  let lists = $state<WaitList[]>([])
  let error = $state('')
  $effect(() => { api.waitlists().then(v => lists = v, e => error = message(e)) })
  function setList(id: string) { if (task.workflow?.type === 'session') { task.workflow.listId = id; task.site = lists.find(l => l.id === id)?.site ?? '' } }
</script>
{#if task.workflow}
  {@const w = task.workflow}
  <section class="workflow">
    {#if w.type === 'session'}
      <h3>Clinic session</h3>
      <label>Site / clinic / provider<select value={w.listId} disabled={stored} onchange={e => setList(e.currentTarget.value)}><option value="">Choose a list</option>{#each lists.filter(l => !l.archived || l.id === w.listId) as l (l.id)}<option value={l.id}>{[l.name,l.site,l.clinic,l.provider].filter(Boolean).join(' · ')}</option>{/each}</select></label>
      <label>Session length (minutes)<input type="number" min="1" max="1440" step="1" bind:value={w.minutes} /></label>
      <p>{w.cancelled ? 'Cancelled session.' : 'The appointment time below is the session start. Checklists can hold preparation steps.'}</p>
    {:else if w.type === 'procurement'}
      <h3>Procurement</h3>
      <div class="grid">
        <label>Progress<select bind:value={w.stage}><option value="request">Request</option><option value="quote">Quote</option><option value="ordered">Ordered / awaiting delivery</option><option value="partial">Part received</option><option value="received">All received</option><option value="invoice">Invoice recorded</option></select></label>
        <label>Supplier reference<input maxlength="80" bind:value={w.supplier} /></label>
        <label>Quote reference / verbal quote note<input maxlength="80" bind:value={w.quoteRef} /></label>
        <label>Order / PO reference<input maxlength="80" bind:value={w.poRef} /></label>
        <label>Ordered quantity<input type="number" min="1" max="1000000" step="1" bind:value={w.quantity} /></label>
        <label>Received so far<input type="number" min="0" max={w.quantity} step="1" bind:value={w.received} /></label>
        <label>Expected delivery<input type="datetime-local" bind:value={() => toInput(w.expected), v => w.expected = fromInput(v)} /></label>
        <label>Invoice reference<input maxlength="80" bind:value={w.invoiceRef} /></label>
      </div>
      <p>Use the chase date below for follow-up. Partial receipts keep the order open; complete it after receipt and invoice are recorded. Undo reverses progress.</p>
    {:else}
      <h3>Delivery</h3>
      <label>Delivery window ends<input type="datetime-local" bind:value={() => toInput(w.windowEnd), v => w.windowEnd = fromInput(v)} /></label>
      <label>Receiving role or initials<input maxlength="80" bind:value={w.receiver} /></label>
      <p>The appointment time below starts the window. Set the site below. Confirm delivery from Sessions &amp; orders when it arrives.</p>
      {#if w.confirmedAt}<p>Confirmed {new Date(w.confirmedAt).toLocaleString()}.</p>{/if}
    {/if}
    {#if error}<p role="alert">{error}</p>{/if}
  </section>
{/if}
<style>.workflow { padding: 14px; background: var(--panel2); border-radius: 12px; display: grid; gap: 12px; } .grid { display: grid; grid-template-columns: repeat(auto-fit,minmax(180px,1fr)); gap: 12px; } label { display: flex; flex-direction: column; gap: 5px; } p { color: var(--muted); font-size: 13px; } h3 { font-size: 16px; }</style>
