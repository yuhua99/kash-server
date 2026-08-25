<script lang="ts">
  import type { Snippet } from "svelte";
  import ListRow from "$lib/ui/ListRow.svelte";
  import MoneyAmount from "$lib/features/money/MoneyAmount.svelte";

  type Props = {
    title: string;
    meta: string;
    amount: number;
    currency: string;
    signed?: boolean;
    tone?: "default" | "income" | "danger";
    actions?: Snippet;
  };

  let { title, meta, amount, currency, signed, tone, actions }: Props = $props();
</script>

<ListRow>
  <div class="ledger-row" class:has-actions={actions}>
    <div class="ledger-row__main">
      <span class="ledger-row__title">{title}</span>
      <span class="ledger-row__meta">{meta}</span>
    </div>
    <MoneyAmount {amount} {currency} {signed} {tone} />
    {#if actions}
      <div class="ledger-row__actions">{@render actions()}</div>
    {/if}
  </div>
</ListRow>

<style>
  .ledger-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-3);
  }

  .ledger-row.has-actions {
    grid-template-columns: minmax(0, 1fr) auto auto;
  }

  .ledger-row__main {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .ledger-row__title {
    overflow: hidden;
    color: var(--text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ledger-row__meta {
    min-width: 0;
    overflow: hidden;
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    letter-spacing: 0.08em;
    text-overflow: ellipsis;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .ledger-row__actions {
    display: flex;
    gap: var(--space-2);
  }

  @media (max-width: 560px) {
    .ledger-row.has-actions {
      grid-template-columns: minmax(0, 1fr) auto;
    }

    .ledger-row.has-actions .ledger-row__actions {
      grid-column: 1 / -1;
      justify-content: flex-end;
    }
  }
</style>
