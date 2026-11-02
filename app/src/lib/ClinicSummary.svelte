<script lang="ts">
  import { api, message, type ClinicSummary } from './api'
  import { app } from './state.svelte'
  let summary = $state<ClinicSummary | null>(null)
  let error = $state('')
  $effect(() => {
    void app.version
    let active = true
    api.clinicSummary().then(s => { if (active) { summary = s; error = '' } }, e => { if (active) error = message(e) })
    return () => { active = false }
  })
  function open() { app.clinicTab = 'refill'; app.view = 'lists' }
</script>

<section class="card clinic">
  <div><h3>Cancelled clinic slots</h3>
    {#if summary}
      <p>{summary.open} open · {summary.filled} filled · {summary.lost} lost</p>
      {#if summary.refillPercent !== null}<p class="muted">{summary.refillPercent.toFixed(0)}% of closed slots refilled{summary.meanFillMinutes !== null ? ` · average ${summary.meanFillMinutes.toFixed(0)} min to fill` : ''}. All recorded slots.</p>{/if}
    {/if}
    {#if error}<p role="alert">{error}</p>{/if}
  </div>
  <button class="btn" onclick={open}>Open clinic desk</button>
</section>
<style>.clinic { padding: 18px; margin: 18px 0; display: flex; gap: 16px; flex-wrap: wrap; align-items: center; justify-content: space-between; } p { margin-top: 6px; } h3 { font-size: 16px; }</style>
