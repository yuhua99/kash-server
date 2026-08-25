<script lang="ts">
  import { amountDisplayMode, formatMoney } from "$lib/features/money/amount-display";

  type Props = {
    amount: number;
    currency: string;
    signed?: boolean;
    tone?: "default" | "income" | "danger";
    plain?: boolean;
  };

  let { amount, currency, signed = false, tone = "default", plain = false }: Props = $props();
</script>

<data class={["money-amount", `money-amount--${tone}`, plain && "money-amount--plain"].filter(Boolean).join(" ")} value={amount}>
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

  .money-amount--plain {
    color: inherit;
    font-family: inherit;
    font-weight: inherit;
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

  .money-amount--plain .money-amount__currency {
    color: inherit;
    font-size: inherit;
    letter-spacing: inherit;
  }
</style>
