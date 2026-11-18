<script lang="ts">
  // Official agent logos (Lobe Icons, MIT). Color logos render as images; one-color logos (Copilot,
  // OpenCode) are drawn with a mask so they follow the theme's text color.
  let { id, size = 16 }: { id: string; size?: number } = $props();
  const MONO = new Set(["copilot", "opencode"]);
  const known = new Set(["claude", "codex", "copilot", "opencode", "antigravity"]);
</script>

{#if known.has(id) && MONO.has(id)}
  <span class="mono" style="width:{size}px;height:{size}px;--src:url('/agents/{id}.svg')" aria-label={id}></span>
{:else if known.has(id)}
  <img src="/agents/{id}.svg" width={size} height={size} alt={id} />
{:else}
  <span class="dot" style="width:{size}px;height:{size}px"></span>
{/if}

<style>
  img { display: block; flex: none; }
  .mono { display: block; flex: none; background: var(--text); -webkit-mask: var(--src) center / contain no-repeat; mask: var(--src) center / contain no-repeat; }
  .dot { display: block; border-radius: 50%; background: var(--text3); flex: none; }
</style>
