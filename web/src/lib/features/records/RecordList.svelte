<script lang="ts">
  import type { components } from "$lib/api/schema";
  import Button from "$lib/ui/Button.svelte";
  import ListRow from "$lib/ui/ListRow.svelte";
  import { amountDisplayMode, formatMoney } from "$lib/features/money/amount-display";
  import type { DateGroup } from "$lib/features/records/view";

  type RecordItem = components["schemas"]["Record"];

  type Props = {
    grouped: boolean;
    groups: DateGroup[];
    records: RecordItem[];
    categoryNames: Map<string, string>;
    limit: number;
    onShowMore: () => void;
    onEdit: (record: RecordItem) => void;
    onDelete: (record: RecordItem) => void;
  };

  let { grouped, groups, records, categoryNames, limit, onShowMore, onEdit, onDelete }: Props = $props();

  const renderedGroups = $derived.by(() => {
    let remaining = limit;
    const result: DateGroup[] = [];

    for (const group of groups) {
      if (remaining <= 0) break;
      const groupRecords = group.records.slice(0, remaining);
      result.push({ ...group, records: groupRecords });
      remaining -= groupRecords.length;
    }

    return result;
  });
  const renderedRecords = $derived(records.slice(0, limit));
  const remaining = $derived(Math.max(records.length - limit, 0));

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
        {formatMoney(record.amount, $amountDisplayMode, record.currency, { signed: true })}
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
    {#each renderedGroups as group (group.date)}
      <section class="group">
        <header class="group__header">
          <span class="group__date">{group.date}</span>
          <span class="group__spend">
            {#each group.spendSummaries as summary (summary.currency)}
              <span class="group__spend-item">{formatMoney(summary.amount, $amountDisplayMode, summary.currency)} {summary.currency}</span>
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
  {#each renderedRecords as record (record.id)}
    {@render row(record)}
  {/each}
{/if}

{#if remaining > 0}
  <div class="show-more">
    <Button variant="secondary" onclick={onShowMore}>Show more ({remaining} remaining)</Button>
  </div>
{/if}

<style>
  .empty {
    padding: var(--space-4);
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
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
    font-size: var(--font-size-xs);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .group__date {
    color: var(--text);
  }

  .group__spend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
    color: var(--text-muted);
  }

  .group__spend-item {
    white-space: nowrap;
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
    font-size: var(--font-size-xs);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .record__amount {
    color: var(--text);
    font-family: var(--font-mono);
    font-weight: 600;
    white-space: nowrap;
  }

  .show-more {
    display: flex;
    justify-content: center;
  }

  .record__amount--income {
    color: var(--success);
  }

  .record__currency {
    color: var(--text-muted);
    font-size: var(--font-size-xs);
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
