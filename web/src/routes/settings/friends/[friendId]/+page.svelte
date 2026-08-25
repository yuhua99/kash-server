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
  import ListRow from "$lib/ui/ListRow.svelte";
  import { toast } from "$lib/ui/toast";
  import { getAcceptedFriendsCached, invalidateFriendsCache } from "$lib/features/friends/cache";
  import { removeFriend, updateNickname } from "$lib/features/friends/api";
  import { notifyFriendsSync } from "$lib/features/friends/sync";
  import { amountDisplayMode, formatMoney } from "$lib/features/money/amount-display";
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
  <h1>{friend?.nickname ?? "Friend"}</h1>

  {#if loading}
    <p role="status">Loading…</p>
  {:else}
    <Block title="Nickname">
      <div class="nickname">
        <input bind:value={nickname} aria-label="Nickname" autocomplete="off" />
        <Button variant="primary" disabled={savingNickname} onclick={saveNickname}>Save</Button>
      </div>
      {#if nicknameError}
        <p role="alert">{nicknameError}</p>
      {/if}
    </Block>

    <Block title="Unsettled">
      {#if shares.length === 0}
        <p class="empty">Nothing to settle.</p>
      {:else}
        <div class="shares">
          {#each shares as share (share.participant_id)}
            <ListRow>
              <div class="share">
                <div class="share__main">
                  <span class="share__desc">{share.description}</span>
                  <span class="share__meta">{share.date} / {share.direction}</span>
                </div>
                <data class="share__amount" value={share.amount}>
                  {formatMoney(share.amount, $amountDisplayMode, share.currency)} {share.currency}
                </data>
              </div>
            </ListRow>
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
      <Button variant="secondary" className="remove" disabled={removing} onclick={remove}>
        {removing ? "Removing" : "Remove friend"}
      </Button>
    </Block>
  {/if}
</section>

<style>
  .page {
    display: grid;
    gap: var(--space-4);
  }

  .nickname {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: var(--space-2);
  }

  .shares {
    border: 1px solid var(--border);
    margin-bottom: var(--space-3);
  }

  .share {
    display: grid;
    min-width: 0;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-3);
  }

  .share__main {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .share__desc {
    min-width: 0;
    overflow: hidden;
    color: var(--text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .share__meta {
    min-width: 0;
    overflow: hidden;
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    letter-spacing: 0.08em;
    text-overflow: ellipsis;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .share__amount {
    color: var(--text);
    font-family: var(--font-mono);
    font-weight: 600;
    white-space: nowrap;
  }

  .empty {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  :global(.remove) {
    border-color: var(--danger);
    color: var(--danger);
  }
</style>
