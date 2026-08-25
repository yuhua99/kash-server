<script lang="ts">
  import type { components } from "$lib/api/schema";
  import Block from "$lib/ui/Block.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import ListRow from "$lib/ui/ListRow.svelte";

  type FriendshipRelation = components["schemas"]["FriendshipRelation"];

  type Props = {
    friends: FriendshipRelation[];
    onSelect?: (friend: FriendshipRelation) => void;
  };

  let { friends, onSelect }: Props = $props();
</script>

<Block title="Friends">
  {#if friends.length === 0}
    <EmptyState message="No friends yet." />
  {:else}
    <div class="friends-list" role="list">
      {#each friends as friend (friend.user_id)}
        <ListRow onclick={() => onSelect?.(friend)}>
          <span class="friends-list__nickname">{friend.nickname}</span>
        </ListRow>
      {/each}
    </div>
  {/if}
</Block>

<style>
  .friends-list {
    border-top: 1px solid var(--border);
  }

  .friends-list__nickname {
    display: block;
    min-width: 0;
    overflow: hidden;
    color: var(--text);
    font-size: var(--font-size-md);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
