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

  async function load() {
    loading = true;
    try {
      const [friends, unsettled] = await Promise.all([
        getAcceptedFriendsCached().catch(() => [] as FriendshipRelation[]),
        listUnsettledShares(friendId, { limit: 1000 }).catch(() => [] as UnsettledShare[]),
      ]);
      friend = friends.find((item) => item.user_id === friendId) ?? null;
      nickname = friend?.nickname ?? "";
      shares = unsettled;
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

</style>
