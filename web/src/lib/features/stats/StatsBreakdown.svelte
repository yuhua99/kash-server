<script lang="ts">
  import Block from "$lib/ui/Block.svelte";
  import { amountDisplayMode, formatMoney } from "$lib/features/money/amount-display";
  import type { BreakdownItem, Totals } from "$lib/features/stats/query";

  type Props = {
    totals: Totals;
    breakdown: BreakdownItem[];
    currency: string;
    note?: string | null;
  };

  let { totals, breakdown, currency, note }: Props = $props();

  let percentages = $derived.by(() => {
    const raw = breakdown.map((item) => item.share * 100);
    const floored = raw.map(Math.floor);
    let remainder = 100 - floored.reduce((a, b) => a + b, 0);
    if (remainder === 100 && floored.every((v) => v === 0)) return floored;
    const diffs = raw.map((v, i) => ({ i, d: v - floored[i] }));
    diffs.sort((a, b) => b.d - a.d);
    for (const { i } of diffs) {
      if (remainder <= 0) break;
      floored[i]++;
      remainder--;
    }
    return floored;
  });
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
          {formatMoney(totals.netTotal, $amountDisplayMode, currency, { signed: true })} {currency}
        </dd>
      </div>
      <div class="totals__cell">
        <dt>Income</dt>
        <dd class="totals__value--income">{formatMoney(totals.incomeTotal, $amountDisplayMode, currency)} {currency}</dd>
      </div>
      <div class="totals__cell">
        <dt>Expense</dt>
        <dd class="totals__value--negative">{formatMoney(totals.expenseTotal, $amountDisplayMode, currency)} {currency}</dd>
      </div>
    </dl>
  </Block>

  <Block title="By category">
    {#if breakdown.length === 0}
      <p class="stats__empty">No data for this period.</p>
    {:else}
      <ul class="breakdown">
        {#each breakdown as item, i (item.categoryId)}
          <li class="breakdown__row">
            <div class="breakdown__head">
              <span class="breakdown__name">{item.name}</span>
              <data class="breakdown__amount" class:breakdown__amount--income={item.isIncome} value={item.total}>
                {formatMoney(item.total, $amountDisplayMode, currency, { signed: true })} {currency}
              </data>
            </div>
            <div class="breakdown__bar" aria-hidden="true">
              <span class="breakdown__fill" style:width={`${percentages[i]}%`}></span>
            </div>
            <span class="breakdown__share">{percentages[i]}%</span>
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
    font-size: var(--font-size-sm);
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
    font-size: var(--font-size-xs);
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
    font-size: var(--font-size-xs);
    letter-spacing: 0.08em;
  }
</style>
