<script lang="ts">
  import { Tabs } from "bits-ui";
  import { handleApiError } from "$lib/api/errors";
  import { validateCategoryName } from "$lib/validation";
  import Block from "$lib/ui/Block.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { toast } from "$lib/ui/toast";
  import { createCategory } from "$lib/features/categories/api";
  import { invalidateCategoriesCache } from "$lib/features/categories/cache";

  type Props = {
    onChange?: () => void;
  };

  let { onChange }: Props = $props();

  let name = $state("");
  let isIncome = $state(false);
  let error = $state("");
  let pending = $state(false);

  async function submit(event: SubmitEvent) {
    event.preventDefault();

    const trimmedName = name.trim();
    const err = validateCategoryName(trimmedName);
    if (err) {
      error = err;
      return;
    }

    error = "";
    pending = true;

    try {
      await createCategory({ name: trimmedName, is_income: isIncome });
      invalidateCategoriesCache();
      toast.success("Category created");
      name = "";
      onChange?.();
    } catch (e) {
      const m = await handleApiError(e, "Could not create category");
      if (m) {
        toast.error(m);
      }
    } finally {
      pending = false;
    }
  }
</script>

<Block title="New category">
  <form class="category-form" onsubmit={submit}>
    <label for="category-name">Name</label>
    <input id="category-name" bind:value={name} oninput={() => (error = "")} disabled={pending} autocomplete="off" />
    {#if error}
      <p role="alert">{error}</p>
    {/if}

    <Tabs.Root
      class="type-tabs"
      value={isIncome ? "income" : "expense"}
      onValueChange={(value) => {
        isIncome = value === "income";
      }}
    >
      <Tabs.List class="type-tabs__list" aria-label="Category type">
        <Tabs.Trigger class="type-tabs__trigger" value="expense" disabled={pending}>Expense</Tabs.Trigger>
        <Tabs.Trigger class="type-tabs__trigger" value="income" disabled={pending}>Income</Tabs.Trigger>
      </Tabs.List>
    </Tabs.Root>

    <Button type="submit" disabled={pending}>{pending ? "Creating" : "Create category"}</Button>
  </form>
</Block>

<style>
  .category-form {
    display: grid;
    gap: var(--space-3);
  }

  label {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  :global(.type-tabs) {
    display: block;
  }

  :global(.type-tabs__list) {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1px;
    border: 1px solid var(--border-strong);
    background: var(--border);
  }

  :global(.type-tabs__trigger) {
    min-height: 40px;
    border: 0;
    background: var(--panel);
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  :global(.type-tabs__trigger[data-state="active"]) {
    background: var(--surface);
    color: var(--accent);
  }

  input:focus {
    outline: none;
    border-color: var(--accent);
  }
</style>
