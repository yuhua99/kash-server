<script lang="ts">
  import { toast } from "./toast";
</script>

{#if $toast.length > 0}
  <div class="toast-stack" role="status" aria-live="polite">
    {#each $toast as t (t.id)}
      <button class={`toast toast-${t.kind}`} type="button" onclick={() => toast.dismiss(t.id)}>
        <span class="kind">{t.kind}</span>
        <span class="message">{t.message}</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .toast-stack {
    position: fixed;
    right: var(--space-4);
    top: var(--space-4);
    z-index: 1000;
    display: grid;
    width: min(calc(100vw - (var(--space-4) * 2)), 360px);
    gap: var(--space-2);
    font-family: var(--font-mono);
  }

  .toast {
    display: grid;
    gap: var(--space-1);
    width: 100%;
    padding: var(--space-3);
    background: var(--panel);
    border: 1px solid currentColor;
    color: var(--text-muted);
    text-align: left;
    font-family: var(--font-mono);
  }

  .toast-success {
    color: var(--success);
  }

  .toast-error {
    color: var(--danger);
  }

  .toast-info {
    color: var(--text-muted);
  }

  .kind {
    font-size: var(--font-size-xs);
    letter-spacing: 0.1em;
    line-height: 1;
    text-transform: uppercase;
  }

  .kind::before {
    content: "[ ";
  }

  .kind::after {
    content: " ]";
  }

  .message {
    color: var(--text);
    font-size: var(--font-size-sm);
    letter-spacing: 0.04em;
  }
</style>
