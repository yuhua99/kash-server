<script lang="ts">
  import { goto } from "$app/navigation";
  import type { components } from "$lib/api/schema";
  import FriendSearch from "$lib/features/friends/FriendSearch.svelte";
  import FriendsList from "$lib/features/friends/FriendsList.svelte";
  import PageHeader from "$lib/ui/PageHeader.svelte";
  import StatusMessage from "$lib/ui/StatusMessage.svelte";
  import { getAcceptedFriendsCached, invalidateFriendsCache } from "$lib/features/friends/cache";
  import { friendsSyncRevision } from "$lib/features/friends/sync";

  type FriendshipRelation = components["schemas"]["FriendshipRelation"];

  let friends = $state<FriendshipRelation[]>([]);
  let loading = $state(true);

  async function refresh() {
    loading = true;
    try {
      friends = await getAcceptedFriendsCached();
    } catch {
      friends = [];
    } finally {
      loading = false;
    }
  }

  function reload() {
    invalidateFriendsCache();
    void refresh();
  }

  $effect(() => {
    void $friendsSyncRevision;
    invalidateFriendsCache();
    void refresh();
  });
</script>

<section class="page">
  <PageHeader title="Friends" />

  <FriendSearch onChange={reload} />

  {#if loading}
    <StatusMessage kind="loading" message="Loading…" />
  {:else}
    <FriendsList {friends} onSelect={(friend) => goto(`/settings/friends/${friend.user_id}`)} />
  {/if}
</section>

