<script lang="ts">
  import type { Snippet } from "svelte";

  type Props = {
    id: string;
    label: string;
    error?: string;
    children: Snippet;
  };

  let { id, label, error, children }: Props = $props();
</script>

<div class="field">
  <label for={id}>{label}</label>
  {@render children()}
  {#if error}
    <p role="alert">{error}</p>
  {/if}
</div>

<style>
  .field {
    display: grid;
    gap: var(--space-2);
  }
</style>
