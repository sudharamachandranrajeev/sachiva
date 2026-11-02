<script lang="ts">
  import { prefs, type Accent, type Motion, type Theme } from './prefs.svelte'
  let { basic = false }: { basic?: boolean } = $props()
</script>
<div class="grid">
  <label class="field">Theme
    <select bind:value={prefs.theme} onchange={() => prefs.apply()}>
      {#each [['system', 'Follow the system'], ['light', 'Calm light'], ['dark', 'Calm dark'], ['glass', 'Blue glass']] as [v, name] (v)}<option value={v as Theme}>{name}</option>{/each}
    </select>
  </label>
  <label class="field">Movement
    <select bind:value={prefs.motion} onchange={() => prefs.apply()}>
      {#each [['system', 'Follow the system'], ['reduced', 'Calm: as little as possible'], ['full', 'Full']] as [v, name] (v)}<option value={v as Motion}>{name}</option>{/each}
    </select>
  </label>
  {#if !basic}
    <label class="field">Accent colour
      <select bind:value={prefs.accent} onchange={() => prefs.apply()}>
        {#each [['theme', 'Match theme'], ['ocean', 'Ocean blue'], ['violet', 'Violet'], ['teal', 'Teal']] as [v, name] (v)}<option value={v as Accent}>{name}</option>{/each}
      </select>
    </label>
  {/if}
</div>
{#if !basic}
  <fieldset>
    <legend>Surface effects</legend>
    <label><input type="checkbox" bind:checked={prefs.depth} onchange={() => prefs.apply()} /> Raised depth</label>
    <label><input type="checkbox" bind:checked={prefs.dropShadow} onchange={() => prefs.apply()} /> Drop shadows</label>
    <label><input type="checkbox" bind:checked={prefs.glow} onchange={() => prefs.apply()} /> Soft glow</label>
  </fieldset>
  <p class="hint">Combine effects as you like. Choices are kept on this device.</p>
{/if}
<style>
  .grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; width: 100%; }
  fieldset { display: flex; gap: 12px 20px; flex-wrap: wrap; border: 0; padding: 4px 0; }
  legend { font-size: 12px; font-weight: 600; color: var(--muted); margin-bottom: 8px; }
  fieldset label { display: flex; align-items: center; gap: 7px; }
  .hint { font-size: 12px; color: var(--muted); }
  @media (max-width: 600px) { .grid { grid-template-columns: 1fr; } }
</style>
