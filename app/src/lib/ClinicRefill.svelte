<script lang="ts">
  import { api, message, type ClinicData, type ClinicEntry, type ClinicSlot, type ClinicCandidate, type ContactOutcome, type Finding } from './api'
  import { app } from './state.svelte'
  import { fromInput, isoDay, toInput, when } from './time'

  let data = $state<ClinicData | null>(null)
  let listId = $state('')
  let slotId = $state('')
  let entryForm = $state<ClinicEntry | null>(null)
  let slotForm = $state<ClinicSlot | null>(null)
  let candidates = $state<ClinicCandidate[]>([])
  let candidatesLoading = $state(false)
  let error = $state('')
  let busy = $state(false)
  let history = $state(false)
  let showArchived = $state(false)
  let note = $state('')
  let outcome = $state<ContactOutcome>('noAnswer')
  let confirmEntry = $state<string | null>(null)
  let findings = $state<Finding[]>([])
  const selected = $derived(data?.slots.find(s => s.id === slotId) ?? null)
  const list = $derived(data?.lists.find(l => l.id === listId))
  const slots = $derived((data?.slots ?? []).filter(s => s.listId === listId && (history || s.state.state === 'open')).sort((a,b) => a.start.localeCompare(b.start)))
  const entries = $derived((data?.entries ?? []).filter(e => e.listId === listId && (showArchived || e.state.state === 'waiting')).sort((a,b) => a.target.localeCompare(b.target)))
  const weekdays = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']
  const ref = (id: string) => data?.entries.find(e => e.id === id)?.reference ?? 'Reference unavailable'
  const outcomeLabel = (v: ContactOutcome) => ({ noAnswer: 'No answer', declined: 'Declined this slot', callBack: 'Call back' }[v])

  async function load() {
    const fresh = await api.clinicData()
    data = fresh
    if (!fresh.lists.some(l => l.id === listId)) listId = fresh.lists.find(l => !l.archived)?.id ?? fresh.lists[0]?.id ?? ''
  }
  $effect(() => { void app.version; load().catch(e => error = message(e)) })
  $effect(() => {
    const id = selected?.id
    void selected?.revision
    void data?.entries
    candidates = []; confirmEntry = null
    if (!id || selected?.state.state !== 'open') return
    let active = true
    candidatesLoading = true
    api.clinicCandidates(id).then(c => { if (active) candidates = c }, e => { if (active) error = message(e) }).finally(() => { if (active) candidatesLoading = false })
    return () => { active = false }
  })
  $effect(() => {
    const text = [entryForm?.reference, entryForm?.appointmentType, slotForm?.appointmentType, note].filter(Boolean).join('\n')
    let active = true
    const timer = setTimeout(() => api.check(text).then(f => { if (active) findings = f }, () => { if (active) findings = [] }), 250)
    return () => { active = false; clearTimeout(timer) }
  })

  async function act(action: () => Promise<unknown>, label: string) {
    busy = true; error = ''
    try { await action(); await load(); app.say(label, { undo: true }) }
    catch (e) { error = message(e) }
    finally { busy = false }
  }
  async function newEntry() {
    try { entryForm = await api.newClinicEntry(listId, '', isoDay(new Date())); slotForm = null; error = '' }
    catch (e) { error = message(e) }
  }
  async function newSlot() {
    try { slotForm = await api.newClinicSlot(listId, new Date(Date.now() + 3_600_000).toISOString()); entryForm = null; error = '' }
    catch (e) { error = message(e) }
  }
  function pick(id: string) { slotId = id; note = ''; confirmEntry = null }
  function changeList(id: string) { listId = id; slotId = ''; entryForm = null; slotForm = null; error = '' }
  function toggleDay(day: number) {
    if (entryForm) entryForm.days = entryForm.days.includes(day) ? entryForm.days.filter(d => d !== day) : [...entryForm.days, day]
  }
  async function saveEntry() {
    if (!entryForm) return
    const value = $state.snapshot(entryForm)
    await act(async () => { await api.saveClinicEntry(value); entryForm = null }, 'Saved the waiting reference.')
  }
  async function saveSlot() {
    if (!slotForm) return
    const value = $state.snapshot(slotForm)
    await act(async () => { const saved = await api.saveClinicSlot(value); slotForm = null; pick(saved.id) }, 'Saved the cancelled slot.')
  }
  async function contact(entryId: string) {
    if (!selected) return
    const slot = selected
    await act(async () => { await api.contactClinicEntry(slot.id, slot.revision, entryId, outcome, note); note = '' }, 'Logged the call attempt.')
  }
  async function close(entryId: string | null) {
    if (!selected) return
    const slot = selected
    await act(() => api.closeClinicSlot(slot.id, slot.revision, entryId), entryId ? 'Slot filled; reference marked booked.' : 'Slot marked lost.')
  }
</script>

<h1>Cancelled slots</h1>
<p class="muted intro">Offer freed appointments to references that fit. Confirm bookings in your booking system first; Sachiva is the follow-up desk.</p>
{#if !data}<p role="status">Loading clinic records…</p>{/if}
{#if error}<p class="error card" role="alert">{error} Reopen the record if it changed elsewhere.</p>{/if}
{#if data && !data.lists.length}
  <div class="card panel"><p>Add a waiting list with its site, clinic and provider initials or role first.</p><button class="btn" onclick={() => app.clinicTab = 'figures'}>Add a list</button></div>
{:else if data}
  <div class="toolbar">
    <label>Site / clinic / provider list<select value={listId} onchange={e => changeList(e.currentTarget.value)}>{#each data.lists as l (l.id)}<option value={l.id}>{[l.name, l.site, l.clinic, l.provider].filter(Boolean).join(' · ')}{l.archived ? ' (archived)' : ''}</option>{/each}</select></label>
    <button class="btn p" disabled={busy || !!list?.archived} onclick={newSlot}>Log cancelled slot</button>
    <button class="btn" disabled={busy || !!list?.archived} onclick={newEntry}>Add waiting reference</button>
  </div>
  {#if findings.length}<div class="warning" role="status"><b>Check for identifying details before saving.</b>{#each findings as f, i (i)}<p>{f.message}</p>{/each}</div>{/if}

  {#if entryForm}
    <form class="card panel" onsubmit={e => { e.preventDefault(); saveEntry() }}>
      <h2>{entryForm.revision ? 'Edit waiting reference' : 'New waiting reference'}</h2>
      <p class="muted">Reference codes only. Never enter a patient name, health number, date of birth or contact details.</p>
      <div class="fields">
        <label>Reference code<input required maxlength="40" readonly={entryForm.revision > 0} bind:value={entryForm.reference} autocomplete="off" /></label>
        <label>Target date<input type="date" required bind:value={entryForm.target} /></label>
        <label>Appointment type (blank = any)<input maxlength="80" bind:value={entryForm.appointmentType} /></label>
        <label>Minutes needed<input type="number" required min="1" max="1440" step="1" bind:value={entryForm.minutes} /></label>
        <label>Earliest time (optional)<input type="time" bind:value={() => entryForm?.earliest?.slice(0,5) ?? '', v => { if (entryForm) entryForm.earliest = v || null }} /></label>
        <label>Finish by (optional)<input type="time" bind:value={() => entryForm?.latest?.slice(0,5) ?? '', v => { if (entryForm) entryForm.latest = v || null }} /></label>
      </div>
      {#if entryForm.revision > 0}<p class="muted">To correct the reference code or move it to another list, archive this entry and add a new one. Preferences and target dates can be edited here.</p>{/if}
      <fieldset><legend>Preferred days (none = any day)</legend><div class="days">{#each weekdays as day, i (day)}<label class="check"><input type="checkbox" checked={entryForm.days.includes(i+1)} onchange={() => toggleDay(i+1)} />{day}</label>{/each}</div></fieldset>
      <div class="actions"><button class="btn p" disabled={busy}>Save reference</button><button type="button" class="btn" onclick={() => entryForm = null}>Cancel</button></div>
    </form>
  {/if}
  {#if slotForm}
    <form class="card panel" onsubmit={e => { e.preventDefault(); saveSlot() }}>
      <h2>{slotForm.revision ? 'Edit cancelled slot' : 'Log cancelled slot'}</h2>
      <div class="fields">
        <label>Appointment date and time<input type="datetime-local" required bind:value={() => toInput(slotForm?.start ?? null), v => { if (slotForm) slotForm.start = fromInput(v) ?? '' }} /></label>
        <label>Slot length (minutes)<input type="number" required min="1" max="1440" step="1" bind:value={slotForm.minutes} /></label>
        <label>Appointment type<input maxlength="80" bind:value={slotForm.appointmentType} /></label>
      </div>
      <div class="actions"><button class="btn p" disabled={busy}>Save slot</button><button type="button" class="btn" onclick={() => slotForm = null}>Cancel</button></div>
    </form>
  {/if}

  <div class="columns">
    <section class="card panel">
      <h2>Slots</h2><label class="check"><input type="checkbox" bind:checked={history} />Include filled and lost</label>
      {#each slots as slot (slot.id)}
        <button class="slot" class:chosen={slot.id === slotId} aria-pressed={slot.id === slotId} onclick={() => pick(slot.id)}><b>{when(slot.start)}</b><span>{slot.minutes} min · {slot.appointmentType || 'Type not specified'} · {slot.state.state}</span></button>
      {:else}<p class="muted">No {history ? '' : 'open '}slots on this list.</p>{/each}
    </section>
    <section class="card panel">
      {#if selected && selected.listId === listId}
        <h2>{when(selected.start)} · {selected.minutes} min</h2>
        <p>{selected.appointmentType || 'Appointment type not specified'} · {selected.state.state}</p>
        {#if selected.state.state === 'open'}
          <div class="actions"><button class="btn" disabled={busy} onclick={() => { slotForm = $state.snapshot(selected); entryForm = null }}>Edit slot</button><button class="btn" disabled={busy} onclick={() => close(null)}>Mark lost</button></div>
          <h3>Who to offer it to</h3><p class="muted">Earliest target date first. Only matching site/clinic/provider, length, type and preferred times appear. Declined references stay off this slot.</p>
          <div class="fields"><label>Call outcome<select bind:value={outcome}><option value="noAnswer">No answer</option><option value="declined">Declined this slot</option><option value="callBack">Call back</option></select></label><label>Call note (reference codes only)<input maxlength="300" bind:value={note} /></label></div>
          {#if candidatesLoading}<p role="status">Finding matching references…</p>{/if}
          {#each candidates as c (c.entry.id)}
            <div class="candidate"><h3>{c.entry.reference}</h3><p class="muted">{c.why}</p>
              <div class="actions"><button class="btn" disabled={busy} onclick={() => contact(c.entry.id)}>Log call</button><button class="btn" disabled={busy} onclick={() => confirmEntry = c.entry.id}>Mark booked…</button></div>
              {#if confirmEntry === c.entry.id}<div class="warning"><p>Confirm {c.entry.reference} has been booked into this slot in the official booking system.</p><div class="actions"><button class="btn p" disabled={busy} onclick={() => close(c.entry.id)}>Booking confirmed — fill slot</button><button class="btn" onclick={() => confirmEntry = null}>Cancel</button></div></div>{/if}
            </div>
          {:else}{#if !candidatesLoading}<p class="muted">No matching waiting references. Check the slot details and waiting preferences; a slot that has started cannot be filled here.</p>{/if}{/each}
        {:else if selected.state.state === 'filled'}<p>Booked reference: <b>{ref(selected.state.entryId)}</b> · {when(selected.state.at)}. Use Undo to correct the last outcome.</p>
        {:else}<p>Lost {when(selected.state.at)}. Use Undo to correct the last outcome.</p>{/if}
        <h3>Call history</h3>
        {#each [...selected.contacts].reverse() as call, i (i)}<div class="call"><b>{ref(call.entryId)}</b> · {outcomeLabel(call.outcome)} · {when(call.at)}{#if call.note}<p>{call.note}</p>{/if}</div>{:else}<p class="muted">No calls logged for this slot.</p>{/each}
      {:else}<h2>Refill a slot</h2><p class="muted">Pick a cancelled slot to see matching references and record what happened.</p>{/if}
    </section>
  </div>
  <section class="card panel">
    <h2>Waiting references</h2><label class="check"><input type="checkbox" bind:checked={showArchived} />Include booked and archived</label>
    {#each entries as entry (entry.id)}<div class="entry"><div><b>{entry.reference}</b> · target {entry.target}<p class="muted">{entry.minutes} min · {entry.appointmentType || 'Any type'} · {entry.state.state}</p></div>
      {#if entry.state.state !== 'booked'}<div class="actions"><button class="btn" disabled={busy} onclick={() => { entryForm = $state.snapshot(entry); slotForm = null }}>Edit</button><button class="btn" disabled={busy} onclick={() => act(() => api.saveClinicEntry({ ...$state.snapshot(entry), state: { state: entry.state.state === 'archived' ? 'waiting' : 'archived' } }), 'Updated the waiting reference.')}>{entry.state.state === 'archived' ? 'Put back' : 'Archive'}</button></div>{/if}
    </div>{:else}<p class="muted">No {showArchived ? '' : 'waiting '}references on this list.</p>{/each}
  </section>
{/if}

<style>
  .intro { margin: 8px 0 18px; } h2 { font-size: 18px; } h3 { font-size: 15px; margin-top: 8px; }
  .toolbar, .actions, .days, .entry { display: flex; flex-wrap: wrap; gap: 10px; align-items: center; }
  .toolbar { align-items: end; margin-bottom: 16px; } .toolbar label { flex: 1; min-width: 220px; }
  .actions { margin-top: 10px; } .panel { padding: 18px; margin-bottom: 16px; display: flex; flex-direction: column; gap: 12px; min-width: 0; }
  .columns { display: grid; grid-template-columns: minmax(220px, 1fr) minmax(0, 2fr); gap: 16px; }
  .fields { display: grid; grid-template-columns: repeat(auto-fit,minmax(180px,1fr)); gap: 12px; }
  label { display: flex; flex-direction: column; gap: 5px; font-size: 13px; } .check { flex-direction: row; align-items: center; } .check input { width: auto; }
  fieldset { border: 0; } legend { margin-bottom: 8px; } .slot { text-align: left; padding: 14px; border: 1px solid var(--line); border-radius: 12px; display: flex; flex-direction: column; gap: 6px; }
  .chosen { background: var(--accent-soft); color: var(--accent); } .candidate, .call, .entry { border-top: 1px solid var(--line); padding-top: 12px; } .entry { justify-content: space-between; }
  .warning { background: var(--warn-soft); border-left: 4px solid var(--warn); padding: 14px; margin: 12px 0; border-radius: 10px; } .error { color: var(--urgent); padding: 14px; margin: 12px 0; }
  @media(max-width: 760px) { .columns { grid-template-columns: 1fr; } }
</style>
