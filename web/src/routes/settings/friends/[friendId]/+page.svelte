<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import type { components } from "$lib/api/schema";
  import { handleApiError } from "$lib/api/errors";
  import { validateNickname } from "$lib/validation";
  import Block from "$lib/ui/Block.svelte";
  import Button from "$lib/ui/Button.svelte";
  import ButtonRow from "$lib/ui/ButtonRow.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import FormField from "$lib/ui/FormField.svelte";
  import PageHeader from "$lib/ui/PageHeader.svelte";
  import StatusMessage from "$lib/ui/StatusMessage.svelte";
  import { toast } from "$lib/ui/toast";
  import { getAcceptedFriendsCached, invalidateFriendsCache } from "$lib/features/friends/cache";
  import { removeFriend, updateNickname } from "$lib/features/friends/api";
  import { notifyFriendsSync } from "$lib/features/friends/sync";
  import LedgerRow from "$lib/features/money/LedgerRow.svelte";
  import MoneyAmount from "$lib/features/money/MoneyAmount.svelte";
  import { convertRecords } from "$lib/features/money/conversion";
  import { getSettingsCached } from "$lib/features/settings/cache";
  import { listUnsettledShares, settleAllWithFriend } from "$lib/features/splits/api";

  type FriendshipRelation = components["schemas"]["FriendshipRelation"];
  type UnsettledShare = components["schemas"]["UnsettledShare"];

  const friendId = $derived(page.params.friendId ?? "");

  let friend = $state<FriendshipRelation | null>(null);
  let nickname = $state("");
  let nicknameError = $state("");
  let shares = $state<UnsettledShare[]>([]);
  let loading = $state(true);
  let savingNickname = $state(false);
  let settling = $state(false);
  let removing = $state(false);
  let mainCurrency = $state("");
  let mainNet = $state<number | null>(null);

  function signedAmount(share: UnsettledShare): number {
    return share.direction === "they_owe_you" ? share.amount : -share.amount;
  }

  const netByCurrency = $derived.by(() => {
    const totals = new Map<string, number>();
    for (const share of shares) {
      totals.set(share.currency, (totals.get(share.currency) ?? 0) + signedAmount(share));
    }
    return [...totals.entries()];
  });

  async function computeMainNet(list: UnsettledShare[]) {
    mainNet = null;
    if (!mainCurrency || list.length === 0) {
      return;
    }
    try {
      if (list.every((share) => share.currency === mainCurrency)) {
        mainNet = list.reduce((total, share) => total + signedAmount(share), 0);
        return;
      }
      const dates = list.map((share) => share.date).sort();
      const items = list.map((share) => ({
        id: share.participant_id,
        amount: signedAmount(share),
        currency: share.currency,
        date: share.date,
      }));
      const { convertedById } = await convertRecords(items, mainCurrency, {
        from: dates[0],
        to: dates[dates.length - 1],
      });
      mainNet = [...convertedById.values()].reduce((total, value) => total + value, 0);
    } catch {
      mainNet = null;
    }
  }

  async function load() {
    loading = true;
    try {
      const [friends, unsettled, settings] = await Promise.all([
        getAcceptedFriendsCached().catch(() => [] as FriendshipRelation[]),
        listUnsettledShares(friendId, { limit: 1000 }).catch(() => [] as UnsettledShare[]),
        getSettingsCached()
          .then((s) => s.main_currency)
          .catch(() => ""),
      ]);
      friend = friends.find((item) => item.user_id === friendId) ?? null;
      nickname = friend?.nickname ?? "";
      shares = unsettled;
      mainCurrency = settings;
      void computeMainNet(unsettled);
    } finally {
      loading = false;
    }
  }

  async function saveNickname() {
    const error = validateNickname(nickname);
    if (error) {
      nicknameError = error;
      return;
    }
    nicknameError = "";
    savingNickname = true;
    try {
      await updateNickname(friendId, nickname.trim() || null);
      invalidateFriendsCache();
      notifyFriendsSync();
      toast.success("Nickname updated");
    } catch (e) {
      const message = await handleApiError(e, "Could not update nickname");
      if (message) {
        toast.error(message);
      }
    } finally {
      savingNickname = false;
    }
  }

  async function settleAll() {
    settling = true;
    try {
      const result = await settleAllWithFriend(friendId);
      toast.success(`Settled ${result.updated_count} share(s)`);
      await load();
    } catch (e) {
      const message = await handleApiError(e, "Could not settle shares");
      if (message) {
        toast.error(message);
      }
    } finally {
      settling = false;
    }
  }

  async function remove() {
    removing = true;
    try {
      await removeFriend(friendId);
      invalidateFriendsCache();
      notifyFriendsSync();
      toast.success("Friend removed");
      await goto("/settings/friends");
    } catch (e) {
      const message = await handleApiError(e, "Could not remove friend");
      if (message) {
        toast.error(message);
      }
    } finally {
      removing = false;
    }
  }

  onMount(load);
</script>

<section class="page">
  <a class="text-link" href="/settings/friends">Back to friends</a>
  <PageHeader title={friend?.nickname ?? "Friend"} />

  {#if loading}
    <StatusMessage kind="loading" message="Loading…" />
  {:else}
    <Block title="Nickname">
      <FormField id="friend-nickname" label="Nickname" error={nicknameError || undefined}>
        <div class="nickname">
          <input id="friend-nickname" bind:value={nickname} autocomplete="off" />
          <Button variant="primary" disabled={savingNickname} onclick={saveNickname}>Save</Button>
        </div>
      </FormField>
    </Block>

    <Block title="Unsettled">
      {#if shares.length === 0}
        <EmptyState message="Nothing to settle." />
      {:else}
        <div class="totals">
          {#if mainNet !== null}
            <div class="totals__row">
              <span class="totals__label">Net</span>
              <MoneyAmount
                amount={mainNet}
                currency={mainCurrency}
                signed
                tone={mainNet < 0 ? "danger" : "income"}
              />
            </div>
          {/if}
          {#if netByCurrency.length > 1 || netByCurrency[0]?.[0] !== mainCurrency}
            {#each netByCurrency as [currency, amount] (currency)}
              <div class="totals__row">
                <span class="totals__label">{currency}</span>
                <MoneyAmount {amount} {currency} signed tone={amount < 0 ? "danger" : "income"} />
              </div>
            {/each}
          {/if}
        </div>
        <div class="shares">
          {#each shares as share (share.participant_id)}
            <LedgerRow
              title={share.description}
              meta={`${share.date} / ${share.direction}`}
              amount={share.amount}
              currency={share.currency}
            />
          {/each}
        </div>
        <ButtonRow>
          <Button variant="primary" disabled={settling} onclick={settleAll}>
            {settling ? "Settling" : "Settle all"}
          </Button>
        </ButtonRow>
      {/if}
    </Block>

    <Block title="Danger zone">
      <Button variant="danger" disabled={removing} onclick={remove}>
        {removing ? "Removing" : "Remove friend"}
      </Button>
    </Block>
  {/if}
</section>

<style>
  .nickname {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: var(--space-2);
  }

  .shares {
    border: 1px solid var(--border);
    margin-bottom: var(--space-3);
  }

  .totals {
    display: grid;
    gap: var(--space-1);
    margin-bottom: var(--space-3);
  }

  .totals__row {
    display: flex;
    justify-content: space-between;
  }

  .totals__label {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

</style>
