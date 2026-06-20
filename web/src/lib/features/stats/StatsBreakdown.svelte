<script lang="ts">
  import Block from "$lib/ui/Block.svelte";
  import { formatMoney, formatSignedMoney } from "$lib/features/money/currency";
  import type { BreakdownItem, Totals } from "$lib/features/stats/query";

  type Props = {
    totals: Totals;
    breakdown: BreakdownItem[];
    currency: string;
    note?: string | null;
  };

  let { totals, breakdown, currency, note }: Props = $props();
</script>

<div class="stats">
  {#if note}
    <p class="stats__note" role="status">{note}</p>
  {/if}

  <Block title="Totals">
    <dl class="totals">
      <div class="totals__cell">
        <dt>Net</dt>
        <dd class:totals__value--negative={totals.netTotal < 0}>
          {formatSignedMoney(totals.netTotal, currency)} {currency}
        </dd>
      </div>
      <div class="totals__cell">
        <dt>Income</dt>
        <dd class="totals__value--income">{formatMoney(totals.incomeTotal, currency)} {currency}</dd>
      </div>
      <div class="totals__cell">
        <dt>Expense</dt>
        <dd class="totals__value--negative">{formatMoney(totals.expenseTotal, currency)} {currency}</dd>
      </div>
    </dl>
  </Block>

  <Block title="By category">
    {#if breakdown.length === 0}
      <p class="stats__empty">No data for this period.</p>
    {:else}
      <ul class="breakdown">
        {#each breakdown as item (item.categoryId)}
          <li class="breakdown__row">
            <div class="breakdown__head">
              <span class="breakdown__name">{item.name}</span>
              <data class="breakdown__amount" class:breakdown__amount--income={item.isIncome} value={item.total}>
                {formatSignedMoney(item.total, currency)} {currency}
              </data>
            </div>
            <div class="breakdown__bar" aria-hidden="true">
              <span class="breakdown__fill" style:width={`${Math.round(item.share * 100)}%`}></span>
            </div>
            <span class="breakdown__share">{Math.round(item.share * 100)}%</span>
          </li>
        {/each}
      </ul>
    {/if}
  </Block>
</div>

<style>
  .stats {
    display: grid;
    gap: var(--space-4);
  }

  .stats__note {
    padding: var(--space-3);
    border: 1px solid var(--border-strong);
    background: var(--surface);
  }

  .stats__empty {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 12px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .totals {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1px;
    background: var(--border);
    border: 1px solid var(--border);
  }

  .totals__cell {
    display: grid;
    gap: var(--space-1);
    padding: var(--space-3);
    background: var(--panel);
  }

  .totals dt {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .totals dd {
    color: var(--text);
    font-family: var(--font-mono);
    font-weight: 600;
  }

  .totals__value--income {
    color: var(--success);
  }

  .totals__value--negative {
    color: var(--danger);
  }

  .breakdown {
    display: grid;
    gap: var(--space-3);
    list-style: none;
  }

  .breakdown__row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: var(--space-1) var(--space-3);
  }

  .breakdown__head {
    display: contents;
  }

  .breakdown__name {
    color: var(--text);
  }

  .breakdown__amount {
    color: var(--text);
    font-family: var(--font-mono);
    font-weight: 600;
    text-align: right;
  }

  .breakdown__amount--income {
    color: var(--success);
  }

  .breakdown__bar {
    grid-column: 1 / -1;
    height: 6px;
    border: 1px solid var(--border);
    background: var(--surface);
  }

  .breakdown__fill {
    display: block;
    height: 100%;
    background: var(--accent);
  }

  .breakdown__share {
    grid-column: 2;
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.08em;
  }
</style>
