<script lang="ts">
  import type { Snippet } from "svelte";

  type Props = {
    title: string;
    children?: Snippet;
  };

  let { title, children }: Props = $props();
</script>

<header class="page-header">
  <h1>{title}</h1>
  {@render children?.()}
</header>

<style>
  .page-header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }
</style>
