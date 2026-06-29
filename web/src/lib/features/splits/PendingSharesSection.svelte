<script lang="ts">
  import type { components } from "$lib/api/schema";
  import Block from "$lib/ui/Block.svelte";
  import ListRow from "$lib/ui/ListRow.svelte";
  import { formatMoney } from "$lib/features/money/currency";

  type PendingShare = components["schemas"]["PendingShare"];

  type Props = {
    shares: PendingShare[];
  };

  let { shares }: Props = $props();
</script>

{#if shares.length > 0}
  <Block title="You owe">
    <div class="pending">
      {#each shares as share (share.participant_id)}
        <ListRow>
          <div class="pending__row">
            <div class="pending__main">
              <span class="pending__desc">{share.description}</span>
              <span class="pending__meta">{share.date} / {share.creditor_name}</span>
            </div>
            <data class="pending__amount" value={share.amount}>
              {formatMoney(share.amount, share.currency)}
              <span class="pending__currency">{share.currency}</span>
            </data>
          </div>
        </ListRow>
      {/each}
    </div>
  </Block>
{/if}

<style>
  .pending {
    border: 1px solid var(--border);
  }

  .pending__row {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: var(--space-3);
  }

  .pending__main {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .pending__desc {
    overflow: hidden;
    color: var(--text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pending__meta {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .pending__amount {
    color: var(--danger);
    font-family: var(--font-mono);
    font-weight: 600;
    white-space: nowrap;
  }

  .pending__currency {
    color: var(--text-muted);
    font-size: var(--font-size-xs);
    letter-spacing: 0.08em;
  }
</style>
