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
    box-shadow: 0 4px 14px rgba(20, 45, 70, .09);
    transition: background .45s cubic-bezier(.2,.8,.2,1), transform .25s ease;
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
