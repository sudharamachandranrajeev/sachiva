<script lang="ts">
  import { api, message, type Task } from './api'
  import { priorityId } from './importance'
  import { app } from './state.svelte'
  let { task }: { task: Task } = $props()
  let busy = $state(false)
  async function change(event: Event) {
    const input = event.currentTarget as HTMLSelectElement
    const value = input.value
    if (value === '__manage') { input.value = priorityId(task); app.managingPriorities = true; return }
    if (value === priorityId(task) || busy) return
    busy = true
    try {
      await api.setImportance(task.id, value)
      await app.refresh()
      app.say(`Priority updated: ${task.title}.`, { undo: true })
    } catch (e) {
      input.value = priorityId(task)
      app.say(message(e), { error: true })
    } finally { busy = false }
  }
</script>
<select aria-label="Priority for {task.title}" value={priorityId(task)} onchange={change} disabled={busy}>
  {#each app.priorityConfig.levels as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
  <option value="__manage">Add or reorder…</option>
</select>
<style>select { max-width: 120px; font-size: 12px; padding: 4px; }</style>
