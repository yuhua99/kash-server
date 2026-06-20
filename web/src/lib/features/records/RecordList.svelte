<script lang="ts">
  import type { components } from "$lib/api/schema";
  import Button from "$lib/ui/Button.svelte";
  import ListRow from "$lib/ui/ListRow.svelte";
  import { amountDisplayMode, formatSignedAmount } from "$lib/features/money/amount-display";
  import { formatMoney } from "$lib/features/money/currency";
  import type { DateGroup } from "$lib/features/records/view";

  type RecordItem = components["schemas"]["Record"];

  type Props = {
    grouped: boolean;
    groups: DateGroup[];
    records: RecordItem[];
    categoryNames: Map<string, string>;
    onEdit: (record: RecordItem) => void;
    onDelete: (record: RecordItem) => void;
  };

  let { grouped, groups, records, categoryNames, onEdit, onDelete }: Props = $props();

  function categoryLabel(record: RecordItem): string {
    if (!record.category_id) {
      return "Uncategorized";
    }
    return categoryNames.get(record.category_id) ?? "Uncategorized";
  }
</script>

{#snippet row(record: RecordItem)}
  <ListRow>
    <div class="record">
      <div class="record__main">
        <span class="record__name">{record.name}</span>
        <span class="record__meta">{categoryLabel(record)} / {record.date}</span>
      </div>
      <data class="record__amount" class:record__amount--income={record.amount > 0} value={record.amount}>
        {formatSignedAmount(record.amount, $amountDisplayMode, record.currency)}
        <span class="record__currency">{record.currency}</span>
      </data>
      <div class="record__actions">
        <Button variant="secondary" size="compact" onclick={() => onEdit(record)}>Edit</Button>
        <Button
          variant="secondary"
          size="compact"
          className="record-delete"
          onclick={() => onDelete(record)}
        >
          Delete
        </Button>
      </div>
    </div>
  </ListRow>
{/snippet}

{#if grouped}
  {#if groups.length === 0}
    <p class="empty">No records in this period.</p>
  {:else}
    {#each groups as group (group.date)}
      <section class="group">
        <header class="group__header">
          <span class="group__date">{group.date}</span>
          <span class="group__spend">
            {#each group.spendSummaries as summary (summary.currency)}
              <span class="group__spend-item">{formatMoney(summary.amount, summary.currency)} {summary.currency}</span>
            {/each}
          </span>
        </header>
        {#each group.records as record (record.id)}
          {@render row(record)}
        {/each}
      </section>
    {/each}
  {/if}
{:else if records.length === 0}
  <p class="empty">No records in this period.</p>
{:else}
  {#each records as record (record.id)}
    {@render row(record)}
  {/each}
{/if}

<style>
  .empty {
    padding: var(--space-4);
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 12px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .group {
    border: 1px solid var(--border);
    margin-bottom: var(--space-3);
  }

  .group__header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .group__date {
    color: var(--text);
  }

  .group__spend {
    display: flex;
    gap: var(--space-3);
    color: var(--text-muted);
  }

  .record {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: var(--space-3);
  }

  .record__main {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .record__name {
    overflow: hidden;
    color: var(--text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .record__meta {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .record__amount {
    color: var(--text);
    font-family: var(--font-mono);
    font-weight: 600;
    white-space: nowrap;
  }

  .record__amount--income {
    color: var(--success);
  }

  .record__currency {
    color: var(--text-muted);
    font-size: 10px;
    letter-spacing: 0.08em;
  }

  .record__actions {
    display: flex;
    gap: var(--space-2);
  }

  :global(.record-delete) {
    border-color: var(--danger);
    color: var(--danger);
  }

  @media (max-width: 560px) {
    .record {
      grid-template-columns: 1fr auto;
    }

    .record__actions {
      grid-column: 1 / -1;
      justify-content: flex-end;
    }
  }
</style>
