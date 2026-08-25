<script lang="ts">
  import type { components } from "$lib/api/schema";
  import Button from "$lib/ui/Button.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import LedgerRow from "$lib/features/money/LedgerRow.svelte";
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
  <LedgerRow
    title={record.name}
    meta={`${categoryLabel(record)} / ${record.date}`}
    amount={record.amount}
    currency={record.currency}
    signed
    tone={record.amount > 0 ? "income" : "default"}
  >
    {#snippet actions()}
      <Button variant="secondary" size="compact" onclick={() => onEdit(record)}>Edit</Button>
      <Button
        variant="danger"
        size="compact"
        onclick={() => onDelete(record)}
      >
        Delete
      </Button>
    {/snippet}
  </LedgerRow>
{/snippet}

{#if grouped}
  {#if groups.length === 0}
    <EmptyState variant="boxed" message="No records in this period." />
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
  <EmptyState variant="boxed" message="No records in this period." />
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

  .show-more {
    display: flex;
    justify-content: center;
  }
</style>
