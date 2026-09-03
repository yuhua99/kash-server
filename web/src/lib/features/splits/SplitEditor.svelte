<script lang="ts">
  import type { components } from "$lib/api/schema";
  import {
    validateSplitParticipantAmount,
    validateSplitTotals,
  } from "$lib/validation";
  import Button from "$lib/ui/Button.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import MoneyAmount from "$lib/features/money/MoneyAmount.svelte";
  import {
    amountInputStep,
    type AmountDisplayMode,
  } from "$lib/features/money/amount-display";
  import {
    assignAllToFriends,
    buildParticipantSplits,
    computeAutoShares,
  } from "$lib/features/splits/allocation";

  type FriendshipRelation = components["schemas"]["FriendshipRelation"];
  export type SplitResult = {
    participants: { user_id: string; amount: number }[];
    valid: boolean;
    error: string | null;
  };

  type Props = {
    friends: FriendshipRelation[];
    total: number;
    mode: AmountDisplayMode;
    currency: string;
    result: SplitResult;
  };

  let {
    friends,
    total,
    mode,
    currency,
    result = $bindable({ participants: [], valid: false, error: null }),
  }: Props = $props();

  let selectedIds = $state<string[]>([]);
  let amounts = $state<Record<string, string>>({});
  let lockedAmounts = $state<Record<string, number>>({});
  let touched = $state<Record<string, boolean>>({});

  function touchedSet(): Set<string> {
    return new Set(Object.keys(touched).filter((id) => touched[id]));
  }

  function recompute() {
    amounts = computeAutoShares({
      selectedIds,
      total,
      lockedAmounts,
      touched: touchedSet(),
      mode,
      currency,
    });
  }

  function toggleFriend(id: string) {
    if (selectedIds.includes(id)) {
      selectedIds = selectedIds.filter((friendId) => friendId !== id);
      touched = { ...touched, [id]: false };
    } else {
      selectedIds = [...selectedIds, id];
    }
    recompute();
  }

  function editAmount(id: string, value: string) {
    amounts = { ...amounts, [id]: value };
    lockedAmounts = { ...lockedAmounts, [id]: Number(value) || 0 };
    touched = { ...touched, [id]: true };
    recompute();
  }

  function assignMax() {
    amounts = assignAllToFriends({ selectedIds, total, mode, currency });
    for (const id of selectedIds) {
      lockedAmounts[id] = Number(amounts[id]) || 0;
      touched[id] = true;
    }
  }

  $effect(() => {
    void total;
    recompute();
  });

  const yourShare = $derived(
    total - selectedIds.reduce((sum, id) => sum + (Number(amounts[id]) || 0), 0),
  );

  $effect(() => {
    const participants = buildParticipantSplits(selectedIds, amounts, mode, currency);
    let error: string | null = null;
    if (selectedIds.length === 0) {
      error = "Select at least one friend.";
    } else {
      for (const participant of participants) {
        const participantError = validateSplitParticipantAmount(participant.amount);
        if (participantError) {
          error = participantError;
          break;
        }
      }
      if (!error) {
        const sum = participants.reduce((acc, participant) => acc + participant.amount, 0);
        error = validateSplitTotals(sum, total);
      }
    }
    result = { participants, valid: error === null, error };
  });
</script>

<div class="split-editor">
  {#if friends.length === 0}
    <EmptyState message="No friends to split with." />
  {:else}
    <ul class="split-list">
      {#each friends as friend (friend.user_id)}
        <li class="split-row">
          <label class="split-check">
            <input
              type="checkbox"
              checked={selectedIds.includes(friend.user_id)}
              onchange={() => toggleFriend(friend.user_id)}
            />
            <span>{friend.nickname}</span>
          </label>
          {#if selectedIds.includes(friend.user_id)}
            <input
              class="split-amount"
              type="number"
              min="0"
              step={amountInputStep(mode, currency)}
              value={amounts[friend.user_id] ?? ""}
              oninput={(event) => editAmount(friend.user_id, (event.currentTarget as HTMLInputElement).value)}
              aria-label={`Amount for ${friend.nickname}`}
            />
          {/if}
        </li>
      {/each}
    </ul>
    <div class="row-split">
      <p class="split-footer meta-label">Your share: <MoneyAmount amount={yourShare} {currency} /></p>
      <Button variant="secondary" size="compact" onclick={assignMax}>Max to friends</Button>
    </div>
  {/if}
</div>

<style>
  .split-editor {
    display: grid;
    gap: var(--space-3);
  }

  .split-list {
    display: grid;
    gap: var(--space-2);
    list-style: none;
  }

  .split-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    min-height: calc(var(--font-size-md) * 1.5 + var(--space-4) + 2px);
    gap: var(--space-3);
  }

  .split-check {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    color: var(--text);
    font-family: inherit;
    font-size: var(--font-size-md);
    letter-spacing: normal;
    text-transform: none;
  }

  .split-check input {
    position: relative;
    width: var(--space-4);
    height: var(--space-4);
    padding: 0;
    border: 1px solid var(--border-strong);
    appearance: none;
    background: var(--surface);
    flex: 0 0 var(--space-4);
  }

  .split-check input:checked {
    border-color: var(--accent);
    background: var(--accent);
  }

  .split-check input:checked::after {
    display: grid;
    position: absolute;
    inset: 0;
    place-items: center;
    color: var(--bg);
    content: "✓";
    font-size: var(--font-size-sm);
    font-weight: 600;
    line-height: 1;
  }

  .split-check input:focus-visible {
    outline: none;
    border-color: var(--accent);
  }

  .split-amount {
    width: 120px;
  }

  .split-footer {
    font-size: var(--font-size-sm);
  }

</style>
