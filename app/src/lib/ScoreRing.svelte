<script lang="ts">
  let { score, size = 72 }: { score: number; size?: number } = $props()
  const tone = $derived(score >= 85 ? 'var(--urgent)' : score >= 60 ? 'var(--warn)' : 'var(--accent)')
</script>

<div
  class="ring"
  style:width="{size}px"
  style:height="{size}px"
  style:font-size="{Math.round(size / 3.6)}px"
  style:background="conic-gradient({tone} 0 {score}%, var(--line) {score}% 100%)"
  role="img"
  aria-label="Priority score {score} out of 99"
>
  <span>{score}</span>
</div>

<style>
  .ring {
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-weight: 600;
    position: relative;
    flex: none;
    transition: background 0.4s;
  }
  .ring::before {
    content: '';
    position: absolute;
    inset: 7px;
    border-radius: 50%;
    background: var(--panel);
  }
  span {
    position: relative;
  }
</style>
