<script lang="ts">
  import type { Snippet } from "svelte";

  type Props = {
    onclick: (event: MouseEvent) => void;
    children?: Snippet;
  };

  let { onclick, children }: Props = $props();
</script>

<button type="button" class="chip" {onclick}>
  {@render children?.()}
</button>

<style>
  .chip {
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    letter-spacing: 0.08em;
  }
</style>
