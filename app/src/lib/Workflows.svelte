<script lang="ts">
  import { api, message, type WorkData, type WorkflowKind, type Finding } from './api'
  import { app } from './state.svelte'
  import { when } from './time'
  let data = $state<WorkData | null>(null)
  let error = $state('')
  let busy = $state(false)
  let completed = $state(false)
  let archived = $state(false)
  let source = $state('')
  let name = $state('')
  let findings = $state<Finding[]>([])
  const jobs = $derived((data?.tasks ?? []).filter(t => t.workflow && (completed || t.status.state !== 'done')))
  async function load() { data = await api.workData(); if (!source) source = data.tasks[0]?.id ?? '' }
  $effect(() => { void app.version; load().catch(e => error = message(e)) })
  $effect(() => { const text = name; const id = setTimeout(() => api.check(text).then(f => findings = f, () => findings = []), 250); return () => clearTimeout(id) })
  async function act(f: () => Promise<unknown>, text: string) { busy = true; error = ''; try { await f(); await load(); app.say(text, { undo: true }) } catch (e) { error = message(e); app.say(error, { error: true }) } finally { busy = false } }
  async function create(kind: WorkflowKind) {
    try { const t = await api.newWorkflowTask(kind, data?.lists.find(l => !l.archived)?.id ?? null); app.editing = { task: t, isNew: true } }
    catch (e) { error = message(e) }
  }
  async function use(id: string) { try { app.editing = { task: await api.taskFromTemplate(id), isNew: true } } catch (e) { error = message(e) } }
  const stage = (s: string) => ({ request: 'Request', quote: 'Quote', ordered: 'Ordered', partial: 'Part received', received: 'All received', invoice: 'Invoice recorded' }[s] ?? s)
</script>
<h1>Sessions &amp; orders</h1>
<p class="intro muted">Clinic sessions, procurement, deliveries and reusable task/checklist templates. Each job uses the normal reminder and undo rules.</p>
<div class="buttons"><button class="btn p" onclick={() => create('session')} disabled={busy || !data?.lists.some(l => !l.archived)}>New clinic session</button><button class="btn" onclick={() => create('procurement')} disabled={busy}>New procurement</button><button class="btn" onclick={() => create('delivery')} disabled={busy}>New delivery</button></div>
{#if error}<p role="alert" class="error">{error}</p>{/if}
{#if !data}<p role="status">Loading workflows…</p>{:else}
  {#if !data.lists.some(l => !l.archived)}<p class="muted">Add a site/clinic/provider list under Waiting-list figures before creating sessions.</p>{/if}
  <label class="check"><input type="checkbox" bind:checked={completed} />Include completed and cancelled jobs</label>
  {#each jobs as task (task.id)}
    {@const w = task.workflow!}
    <article class="card job"><h2>{task.title}</h2><p>{w.type === 'session' ? 'Clinic session' : w.type === 'delivery' ? 'Delivery' : 'Procurement'} · {task.status.state === 'done' ? 'done' : 'open'}{task.site ? ` · ${task.site}` : ''}</p>
      {#if w.type === 'session'}<p>{data.lists.find(l => l.id === w.listId)?.name ?? 'Clinic'} · {when(task.appointment)} · {w.minutes} min{w.cancelled ? ' · cancelled' : ''}</p>
      {:else if w.type === 'procurement'}<p>{stage(w.stage)} · {w.received} of {w.quantity} received{w.poRef ? ` · ${w.poRef}` : ''}</p>{#if w.expected}<p>Expected {when(w.expected)}</p>{/if}{#if task.due}<p>Chase {when(task.due)}</p>{/if}
      {:else}<p>{when(task.appointment)} – {when(w.windowEnd)} · {w.receiver}</p>{#if w.confirmedAt}<p>Confirmed {when(w.confirmedAt)}</p>{/if}{/if}
      <div class="buttons"><button class="btn" onclick={() => app.edit(task)}>Open job</button>
        {#if task.status.state !== 'done' && w.type === 'delivery'}<button class="btn p" disabled={busy} onclick={() => act(() => api.confirmDelivery(task.id), 'Delivery confirmed.')}>Confirm delivered</button>{/if}
        {#if task.status.state !== 'done' && w.type === 'session'}<button class="btn" disabled={busy} onclick={() => act(() => api.cancelSession(task.id), 'Session cancelled.')}>Cancel session</button>{/if}
      </div>
    </article>
  {:else}<p class="muted">No {completed ? '' : 'open '}workflow jobs yet.</p>{/each}
  <section class="card templates"><h2>Reusable templates</h2><p class="muted">Save any existing job and its checklist. Using a template creates a fresh job: checklist ticks, old dates, receipts and confirmations start clear. Set new dates before saving.</p>
    <form onsubmit={e => { e.preventDefault(); act(async () => { await api.createTemplate(source, name); name = '' }, 'Template saved.') }}>
      <label>Copy this job<select bind:value={source}><option value="">Choose a saved job</option>{#each data.tasks as task (task.id)}<option value={task.id}>{task.title}</option>{/each}</select></label>
      <label>Template name<input maxlength="80" required bind:value={name} /></label>
      <button class="btn p" disabled={busy || !source || !name.trim()}>Save template</button>
    </form>
    {#each findings as f, i (i)}<p class="warning" role="status">{f.message}</p>{/each}
    <label class="check"><input type="checkbox" bind:checked={archived} />Show archived templates</label>
    {#each data.templates.filter(t => archived || !t.archived) as template (template.id)}<div class="template"><b>{template.name}{template.archived ? ' (archived)' : ''}</b><div class="buttons"><button class="btn" disabled={busy || template.archived} onclick={() => use(template.id)}>Use template</button><button class="btn" disabled={busy} onclick={() => act(() => api.archiveTemplate(template.id, template.revision, !template.archived), 'Template updated.')}>{template.archived ? 'Put back' : 'Archive'}</button></div></div>{:else}<p class="muted">No templates yet.</p>{/each}
  </section>
{/if}
<style>.intro { margin: 8px 0 16px; } .buttons { display:flex;flex-wrap:wrap;gap:8px;margin:12px 0; } .job,.templates { padding:18px;margin:16px 0;display:grid;gap:10px; } h2 {font-size:18px;} label {display:flex;flex-direction:column;gap:5px;} .check {flex-direction:row;align-items:center;margin:14px 0;} .check input{width:auto;} form {display:grid;gap:12px;max-width:600px;} .template {border-top:1px solid var(--line);padding-top:12px;display:flex;flex-wrap:wrap;align-items:center;justify-content:space-between;gap:12px;} .error{color:var(--urgent);} .warning{color:var(--warn);}</style>
