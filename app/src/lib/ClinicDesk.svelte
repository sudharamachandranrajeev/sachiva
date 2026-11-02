<script lang="ts">
  import { app } from './state.svelte'
  import WaitingLists from './WaitingLists.svelte'
  import ClinicRefill from './ClinicRefill.svelte'
  import Workflows from './Workflows.svelte'
</script>

<nav class="tabs" aria-label="Clinic desk">
  <button class="btn" class:p={app.clinicTab === 'figures'} aria-pressed={app.clinicTab === 'figures'} onclick={() => app.clinicTab = 'figures'}>Waiting-list figures</button>
  <button class="btn" class:p={app.clinicTab === 'refill'} aria-pressed={app.clinicTab === 'refill'} onclick={() => app.clinicTab = 'refill'}>Cancelled slots</button>
  <button class="btn" class:p={app.clinicTab === 'work'} aria-pressed={app.clinicTab === 'work'} onclick={() => app.clinicTab = 'work'}>Sessions &amp; orders</button>
</nav>
{#if app.clinicTab === 'figures'}<WaitingLists />{:else if app.clinicTab === 'refill'}<ClinicRefill />{:else}<Workflows />{/if}

<style>.tabs { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 20px; }</style>
