<script lang="ts">
  import type { components } from "$lib/api/schema";
  import { handleApiError } from "$lib/api/errors";
  import { invalidateFriendsCache } from "$lib/features/friends/cache";
  import { searchUsers, sendFriendRequest } from "$lib/features/friends/api";
  import { notifyFriendsSync } from "$lib/features/friends/sync";
  import { validateFriendSearchQuery } from "$lib/validation";
  import Block from "$lib/ui/Block.svelte";
  import Button from "$lib/ui/Button.svelte";
  import FormField from "$lib/ui/FormField.svelte";
  import ListRow from "$lib/ui/ListRow.svelte";
  import { toast } from "$lib/ui/toast";

  type PublicUser = components["schemas"]["PublicUser"];

  type Props = {
    onChange?: () => void;
  };

  let { onChange }: Props = $props();
  let query = $state("");
  let results = $state<PublicUser[]>([]);
  let error = $state("");
  let searching = $state(false);
  let sendingId = $state<string | null>(null);

  async function handleSearch(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    error = "";

    const validationError = validateFriendSearchQuery(query);
    if (validationError) {
      error = validationError;
      return;
    }

    searching = true;
    try {
      results = await searchUsers({ query: query.trim(), limit: 20 });
    } catch (err) {
      results = [];
      const message = await handleApiError(err, "Unable to search users.");
      if (message) {
        toast.error(message);
      }
    } finally {
      searching = false;
    }
  }

  async function handleSendRequest(user: PublicUser): Promise<void> {
    sendingId = user.id;
    try {
      await sendFriendRequest(user.username);
      toast.success("Request sent");
      invalidateFriendsCache();
      notifyFriendsSync();
      onChange?.();
    } catch (err) {
      const message = await handleApiError(err, "Unable to send request.");
      if (message) {
        toast.error(message);
      }
    } finally {
      sendingId = null;
    }
  }
</script>

<Block title="Find friends">
  <form onsubmit={handleSearch}>
    <FormField id="friend-search-query" label="Username" error={error || undefined}>
      <div class="friend-search__controls">
        <input id="friend-search-query" type="search" bind:value={query} autocomplete="off" />
        <Button type="submit" disabled={searching}>{searching ? "Searching" : "Search"}</Button>
      </div>
    </FormField>
  </form>

  <div class="friend-search__results" role="list">
    {#each results as result (result.id)}
      <ListRow>
        <div class="friend-search__row">
          <span class="friend-search__username">{result.username}</span>
          <Button
            size="compact"
            disabled={sendingId === result.id}
            onclick={() => handleSendRequest(result)}
          >
            {sendingId === result.id ? "Sending" : "Add"}
          </Button>
        </div>
      </ListRow>
    {/each}
  </div>
</Block>

<style>
  .friend-search__controls {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: var(--space-2);
  }

  .friend-search__results {
    margin-top: var(--space-3);
    border-top: 1px solid var(--border);
  }

  .friend-search__row {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: var(--space-3);
  }

  .friend-search__username {
    color: var(--text);
    font-size: var(--font-size-md);
  }
</style>
