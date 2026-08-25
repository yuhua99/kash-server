<script lang="ts">
  type Props = {
    message: string;
    variant?: "boxed" | "inline";
  };

  let { message, variant = "inline" }: Props = $props();
</script>

<p class={["empty-state", `empty-state--${variant}`].join(" ")}>{message}</p>

<style>
  .empty-state {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .empty-state--boxed {
    padding: var(--space-4);
    border: 1px solid var(--border);
  }
</style>
