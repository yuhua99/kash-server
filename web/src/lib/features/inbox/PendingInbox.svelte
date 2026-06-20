<script lang="ts">
  import { handleApiError } from "$lib/api/errors";
  import type { components } from "$lib/api/schema";
  import Button from "$lib/ui/Button.svelte";
  import ButtonRow from "$lib/ui/ButtonRow.svelte";
  import Dialog from "$lib/ui/Dialog.svelte";
  import SelectField from "$lib/ui/SelectField.svelte";
  import { toast } from "$lib/ui/toast";
  import { getCategoriesCached } from "$lib/features/categories/cache";
  import { formatMoney } from "$lib/features/money/currency";
  import {
    acceptPendingFriend,
    declinePendingFriend,
    isAlreadyHandled,
    loadPendingInbox,
    savePendingShare,
    type InboxItem,
  } from "$lib/features/inbox/data";

  type Category = components["schemas"]["Category"];

  type Props = {
    userId: string | null;
  };

  let { userId }: Props = $props();

  let queue = $state<InboxItem[]>([]);
  let categories = $state<Category[]>([]);
  let selectedCategoryId = $state("");
  let busy = $state(false);
  let bootstrappedUserId = $state<string | null>(null);

  const head = $derived(queue[0] ?? null);
  const open = $derived(head !== null);

  const categoryItems = $derived(categories.map((category) => ({ value: category.id, label: category.name })));

  $effect(() => {
    if (userId && userId !== bootstrappedUserId) {
      bootstrappedUserId = userId;
      void bootstrap();
    }
  });

  async function bootstrap() {
    queue = await loadPendingInbox();
    if (queue.some((item) => item.kind === "share")) {
      const all = await getCategoriesCached().catch(() => [] as Category[]);
      categories = [...all].sort((a, b) => Number(a.is_income) - Number(b.is_income));
      selectedCategoryId = categories[0]?.id ?? "";
    }
  }

  function dismissHead() {
    queue = queue.slice(1);
  }

  async function runAction(action: () => Promise<void>, fallback: string) {
    busy = true;
    try {
      await action();
      dismissHead();
    } catch (error) {
      if (isAlreadyHandled(error)) {
        toast.info("Already handled");
        dismissHead();
        return;
      }
      const message = await handleApiError(error, fallback);
      if (message) {
        toast.error(message);
      }
    } finally {
      busy = false;
    }
  }

  function acceptFriendItem(friendUserId: string) {
    void runAction(() => acceptPendingFriend(friendUserId), "Could not accept request");
  }

  function declineFriendItem(friendUserId: string) {
    void runAction(() => declinePendingFriend(friendUserId), "Could not decline request");
  }

  function saveShareItem(participantId: string) {
    if (!selectedCategoryId) {
      toast.error("Select a category");
      return;
    }
    void runAction(() => savePendingShare(participantId, selectedCategoryId), "Could not save share");
  }
</script>

{#if head?.kind === "friend"}
  <Dialog {open} title="Friend request" description={`${head.friend.nickname} wants to connect.`}>
    <ButtonRow>
      <Button variant="secondary" disabled={busy} onclick={() => declineFriendItem(head.friend.user_id)}>Decline</Button>
      <Button variant="primary" disabled={busy} onclick={() => acceptFriendItem(head.friend.user_id)}>Accept</Button>
    </ButtonRow>
  </Dialog>
{:else if head?.kind === "share"}
  <Dialog {open} title="Record a shared expense" description={head.share.description}>
    <dl class="share">
      <div><dt>From</dt><dd>{head.share.creditor_name}</dd></div>
      <div><dt>Date</dt><dd>{head.share.date}</dd></div>
      <div><dt>Amount</dt><dd>{formatMoney(head.share.amount, head.share.currency)} {head.share.currency}</dd></div>
    </dl>
    {#if categories.length === 0}
      <p role="alert">No categories available. Create one first.</p>
    {:else}
      <SelectField
        id="inbox-share-category"
        label="Category"
        value={selectedCategoryId}
        items={categoryItems}
        onValueChange={(value) => (selectedCategoryId = value)}
      />
    {/if}
    <ButtonRow>
      <Button
        variant="primary"
        disabled={busy || categories.length === 0}
        onclick={() => saveShareItem(head.share.participant_id)}
      >
        {busy ? "Saving" : "Save"}
      </Button>
    </ButtonRow>
  </Dialog>
{/if}

<style>
  .share {
    display: grid;
    gap: var(--space-2);
    margin: var(--space-4) 0;
  }

  .share div {
    display: grid;
    grid-template-columns: 80px 1fr;
    gap: var(--space-3);
  }

  .share dt {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .share dd {
    color: var(--text);
    font-family: var(--font-mono);
  }
</style>
