<script lang="ts">
  import type { Snippet } from "svelte";

  type Props = {
    children?: Snippet;
  };

  let { children }: Props = $props();
</script>

<div class="button-row">
  {@render children?.()}
</div>

<style>
  .button-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }
</style>
