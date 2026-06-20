<script lang="ts">
  import type { Snippet } from "svelte";

  type Props = {
    children?: Snippet;
    onclick?: (event: MouseEvent) => void;
  };

  let { children, onclick }: Props = $props();
</script>

{#if onclick}
  <button type="button" class="list-row list-row--button" {onclick}>
    {@render children?.()}
  </button>
{:else}
  <div class="list-row">
    {@render children?.()}
  </div>
{/if}

<style>
  .list-row {
    display: block;
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-bottom: 1px solid var(--border);
    background: transparent;
    color: var(--text);
    font: inherit;
    text-align: left;
  }

  .list-row:hover {
    background: var(--surface);
  }

  .list-row--button {
    cursor: pointer;
  }

  .list-row--button:focus-visible {
    outline: none;
    border-color: var(--accent);
  }
</style>
