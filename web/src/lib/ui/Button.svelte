<script lang="ts">
  import type { Snippet } from "svelte";

  type Props = {
    variant?: "primary" | "secondary" | "danger";
    size?: "compact";
    type?: "button" | "submit" | "reset";
    disabled?: boolean;
    onclick?: (event: MouseEvent) => void;
    children?: Snippet;
  };

  let {
    variant = "primary",
    size,
    type = "button",
    disabled = false,
    onclick,
    children,
  }: Props = $props();
</script>

<button
  class={`btn btn-${variant}${size === "compact" ? " btn--compact" : ""}`}
  {type}
  {disabled}
  {onclick}
>
  {@render children?.()}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 40px;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--accent);
    background: transparent;
    color: var(--accent);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    font-weight: 600;
    letter-spacing: 0.08em;
    line-height: 1;
    text-transform: uppercase;
    cursor: pointer;
  }

  .btn:focus-visible {
    outline: none;
    border-color: var(--accent);
  }

  .btn:disabled {
    cursor: not-allowed;
  }

  .btn-primary {
    border-color: var(--accent);
    background: var(--surface);
    color: var(--accent);
  }

  .btn-secondary {
    border-color: var(--border-strong);
    background: transparent;
    color: var(--text-muted);
  }

  .btn-danger {
    border-color: var(--danger);
    color: var(--danger);
  }

  .btn--compact {
    min-height: 36px;
    padding: var(--space-2) var(--space-3);
    font-size: var(--font-size-xs);
  }
</style>
