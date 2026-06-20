<script lang="ts">
  import type { Snippet } from "svelte";

  type Props = {
    title: string;
    children?: Snippet;
  };

  let { title, children }: Props = $props();
</script>

<section class="block">
  <header class="block__header">
    <h2 class="block__title">{title}</h2>
  </header>
  <div class="block__body">
    {@render children?.()}
  </div>
</section>

<style>
  .block {
    border: 1px solid var(--border);
    background: transparent;
  }

  .block__header {
    border-bottom: 1px solid var(--border);
    padding: var(--space-2) var(--space-3);
  }

  .block__title {
    margin: 0;
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.08em;
    line-height: 1;
    text-transform: uppercase;
  }

  .block__title::before {
    content: "[ ";
  }

  .block__title::after {
    content: " ]";
  }

  .block__body {
    padding: var(--space-3);
  }
</style>
