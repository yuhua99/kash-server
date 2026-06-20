<script lang="ts">
  import { Select } from "bits-ui";

  type OptionItem = {
    value: string;
    label: string;
  };

  type SeparatorItem = {
    kind: "separator";
  };

  type SelectItem = OptionItem | SeparatorItem;

  type Props = {
    id: string;
    value: string;
    label: string;
    items: SelectItem[];
    disabled?: boolean;
    onValueChange: (value: string) => void;
  };

  let { id, value, label, items, disabled = false, onValueChange }: Props = $props();

  let optionItems = $derived(items.filter((item): item is OptionItem => !("kind" in item)));
  let selectedLabel = $derived(optionItems.find((option) => option.value === value)?.label ?? "");
</script>

<div class="select-field">
  <label for={id}>{label}</label>
  <Select.Root type="single" {value} {onValueChange} {disabled} items={optionItems}>
    <Select.Trigger {id} class="kash-select-trigger">{selectedLabel || "Select"}</Select.Trigger>
    <Select.Portal>
      <Select.Content class="kash-select-content">
        {#each items as item, i (i)}
          {#if "kind" in item}
            <div class="kash-select-sep" role="separator"></div>
          {:else}
            <Select.Item value={item.value} label={item.label} class="kash-select-item">
              {item.label}
            </Select.Item>
          {/if}
        {/each}
      </Select.Content>
    </Select.Portal>
  </Select.Root>
</div>

<style>
  .select-field {
    display: grid;
    gap: var(--space-2);
  }

  .select-field label {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  :global(.kash-select-trigger) {
    width: 100%;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    padding: var(--space-3) var(--space-4);
    font-family: var(--font-mono);
    font-size: 0.8125rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-align: left;
    text-transform: uppercase;
  }

  :global(.kash-select-trigger:focus-visible) {
    border-color: var(--accent);
    outline: none;
  }

  :global(.kash-select-trigger:disabled) {
    color: var(--text-dim);
    cursor: not-allowed;
  }

  :global(.kash-select-content) {
    z-index: 60;
    min-width: var(--bits-select-anchor-width);
    border: 1px solid var(--border);
    background: var(--panel);
    padding: var(--space-1);
  }

  :global(.kash-select-item) {
    padding: var(--space-2) var(--space-3);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 0.8125rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  :global(.kash-select-item[data-highlighted]),
  :global(.kash-select-item[data-selected]) {
    background: var(--panel-strong);
    color: var(--accent);
    outline: none;
  }

  :global(.kash-select-sep) {
    height: 1px;
    margin: var(--space-1) 0;
    background: var(--border);
  }
</style>
