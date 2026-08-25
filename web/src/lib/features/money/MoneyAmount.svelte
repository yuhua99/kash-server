<script lang="ts">
  import { amountDisplayMode, formatMoney } from "$lib/features/money/amount-display";

  type Props = {
    amount: number;
    currency: string;
    signed?: boolean;
    tone?: "default" | "income" | "danger";
  };

  let { amount, currency, signed = false, tone = "default" }: Props = $props();
</script>

<data class={["money-amount", `money-amount--${tone}`].join(" ")} value={amount}>
  {formatMoney(amount, $amountDisplayMode, currency, { signed })}
  <span class="money-amount__currency">{currency}</span>
</data>

<style>
  .money-amount {
    color: var(--text);
    font-family: var(--font-mono);
    font-weight: 600;
    white-space: nowrap;
  }

  .money-amount--income {
    color: var(--success);
  }

  .money-amount--danger {
    color: var(--danger);
  }

  .money-amount__currency {
    color: var(--text-muted);
    font-size: var(--font-size-xs);
    letter-spacing: 0.08em;
  }
</style>
