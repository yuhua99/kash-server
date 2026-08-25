<script lang="ts">
  import { handleApiError } from "$lib/api/errors";
  import type { components } from "$lib/api/schema";
  import Button from "$lib/ui/Button.svelte";
  import ButtonRow from "$lib/ui/ButtonRow.svelte";
  import Dialog from "$lib/ui/Dialog.svelte";
  import SelectField from "$lib/ui/SelectField.svelte";
  import { toast } from "$lib/ui/toast";
  import { getCategoriesCached } from "$lib/features/categories/cache";
  import MoneyAmount from "$lib/features/money/MoneyAmount.svelte";
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
    try {
      queue = await loadPendingInbox();
      if (queue.some((item) => item.kind === "share")) {
        const all = await getCategoriesCached();
        categories = [...all].sort((a, b) => Number(a.is_income) - Number(b.is_income));
      }
    } catch (error) {
      queue = [];
      categories = [];
      const message = await handleApiError(error, "Could not load inbox");
      if (message) {
        toast.error(message);
      }
    }
  }

  function dismissHead() {
    queue = queue.slice(1);
    selectedCategoryId = "";
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
    void runAction(() => savePendingShare(participantId, selectedCategoryId), "Could not save share");
  }
</script>

{#if head?.kind === "friend"}
  <Dialog {open} onOpenChange={(o) => { if (!o && !busy) dismissHead(); }} title="Friend request" description={`${head.friend.nickname} wants to connect.`}>
    <ButtonRow>
      <Button variant="secondary" disabled={busy} onclick={() => declineFriendItem(head.friend.user_id)}>Decline</Button>
      <Button variant="primary" disabled={busy} onclick={() => acceptFriendItem(head.friend.user_id)}>Accept</Button>
    </ButtonRow>
  </Dialog>
{:else if head?.kind === "share"}
  <Dialog {open} onOpenChange={(o) => { if (!o && !busy) dismissHead(); }} title="Record a shared expense" description={head.share.description}>
    <div class="share-form">
      <dl class="share">
        <div><dt>From</dt><dd>{head.share.creditor_name}</dd></div>
        <div><dt>Date</dt><dd>{head.share.date}</dd></div>
        <div><dt>Amount</dt><dd><MoneyAmount amount={head.share.amount} currency={head.share.currency} /></dd></div>
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
          disabled={busy || categories.length === 0 || !selectedCategoryId}
          onclick={() => saveShareItem(head.share.participant_id)}
        >
          {busy ? "Saving" : "Save"}
        </Button>
      </ButtonRow>
    </div>
  </Dialog>
{/if}

<style>
  .share-form {
    display: grid;
    gap: var(--space-3);
    margin-top: var(--space-4);
  }

  .share {
    display: grid;
    gap: var(--space-2);
  }

  .share div {
    display: grid;
    grid-template-columns: 80px 1fr;
    gap: var(--space-3);
  }

  .share dt {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .share dd {
    color: var(--text);
    font-family: var(--font-mono);
  }
</style>
