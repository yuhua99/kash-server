<script lang="ts">
  import type { Snippet } from "svelte";
  import { Dialog } from "bits-ui";

  type Props = {
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    title: string;
    description?: string;
    children?: Snippet;
  };

  let { open = $bindable(false), onOpenChange, title, description, children }: Props = $props();
</script>

<Dialog.Root bind:open {onOpenChange}>
  <Dialog.Portal>
    <Dialog.Overlay class="kash-dialog-overlay" />
    <Dialog.Content class="kash-dialog-content">
      <Dialog.Title class="kash-dialog-title">{title}</Dialog.Title>
      {#if description}
        <Dialog.Description class="kash-dialog-desc">{description}</Dialog.Description>
      {/if}
      {@render children?.()}
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>

<style>
  :global(.kash-dialog-overlay) {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: color-mix(in srgb, var(--bg) 80%, transparent);
  }

  :global(.kash-dialog-content) {
    position: fixed;
    top: 50%;
    left: 50%;
    z-index: 51;
    width: min(92vw, 480px);
    max-width: 480px;
    transform: translate(-50%, -50%);
    border: 1px solid var(--border-strong);
    background: var(--panel);
    padding: var(--space-6);
  }

  :global(.kash-dialog-title) {
    margin: 0;
    color: var(--text);
    font-family: var(--font-display);
    font-size: 1rem;
    font-weight: 800;
    line-height: 1;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  :global(.kash-dialog-desc) {
    margin: var(--space-3) 0 var(--space-4);
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 0.75rem;
    line-height: 1.5;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }
</style>
