<script lang="ts">
  import { Tabs } from "bits-ui";

  type Item = {
    value: string;
    label: string;
  };

  type Props = {
    items: Item[];
    value: string;
    onValueChange: (v: string) => void;
    ariaLabel: string;
    disabled?: boolean;
  };

  let { items, value, onValueChange, ariaLabel, disabled = false }: Props = $props();
</script>

<Tabs.Root value={value} {onValueChange}>
  <Tabs.List class="segmented-control__list" aria-label={ariaLabel} style={`grid-template-columns: repeat(${items.length}, 1fr)`}>
    {#each items as item (item.value)}
      <Tabs.Trigger class="segmented-control__trigger" value={item.value} {disabled}>{item.label}</Tabs.Trigger>
    {/each}
  </Tabs.List>
</Tabs.Root>

<style>
  :global(.segmented-control__list) {
    display: grid;
    gap: 1px;
    border: 1px solid var(--border-strong);
    background: var(--border);
  }

  :global(.segmented-control__trigger) {
    min-height: 40px;
    border: 0;
    background: var(--panel);
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  :global(.segmented-control__trigger[data-state="active"]) {
    background: var(--surface);
    color: var(--accent);
  }
</style>
