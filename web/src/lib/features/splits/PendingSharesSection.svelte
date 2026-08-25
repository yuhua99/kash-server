<script lang="ts">
  import type { components } from "$lib/api/schema";
  import Block from "$lib/ui/Block.svelte";
  import LedgerRow from "$lib/features/money/LedgerRow.svelte";

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
        <LedgerRow
          title={share.description}
          meta={`${share.date} / ${share.creditor_name}`}
          amount={share.amount}
          currency={share.currency}
          tone="danger"
        />
      {/each}
    </div>
  </Block>
{/if}

<style>
  .pending {
    border: 1px solid var(--border);
  }

</style>
