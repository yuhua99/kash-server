<script lang="ts">
  import { handleApiError } from "$lib/api/errors";
  import type { components } from "$lib/api/schema";
  import { validateCategoryName } from "$lib/validation";
  import Button from "$lib/ui/Button.svelte";
  import ButtonRow from "$lib/ui/ButtonRow.svelte";
  import Dialog from "$lib/ui/Dialog.svelte";
  import { toast } from "$lib/ui/toast";
  import { updateCategory } from "$lib/features/categories/api";
  import { invalidateCategoriesCache } from "$lib/features/categories/cache";

  type Category = components["schemas"]["Category"];

  type Props = {
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    category: Category | null;
    onSaved?: () => void;
  };

  let { open = $bindable(false), onOpenChange, category, onSaved }: Props = $props();

  let name = $state("");
  let error = $state("");
  let pending = $state(false);

  $effect(() => {
    name = category?.name ?? "";
    error = "";
  });

  function closeDialog() {
    open = false;
    onOpenChange?.(false);
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();

    if (!category) {
      return;
    }

    const trimmedName = name.trim();
    const err = validateCategoryName(trimmedName);
    if (err) {
      error = err;
      return;
    }

    error = "";
    pending = true;

    try {
      await updateCategory(category.id, { name: trimmedName });
      invalidateCategoriesCache();
      toast.success("Category updated");
      onSaved?.();
      closeDialog();
    } catch (e) {
      const m = await handleApiError(e, "Could not update category");
      if (m) {
        toast.error(m);
      }
    } finally {
      pending = false;
    }
  }
</script>

<Dialog bind:open {onOpenChange} title="Edit category">
  <form class="edit-form" onsubmit={save}>
    <label for="edit-category-name">Name</label>
    <input id="edit-category-name" bind:value={name} oninput={() => (error = "")} disabled={pending} autocomplete="off" />
    {#if error}
      <p role="alert">{error}</p>
    {/if}
    <ButtonRow>
      <Button variant="secondary" type="button" disabled={pending} onclick={closeDialog}>Cancel</Button>
      <Button type="submit" disabled={pending || !category}>{pending ? "Saving" : "Save"}</Button>
    </ButtonRow>
  </form>
</Dialog>

<style>
  .edit-form {
    display: grid;
    gap: var(--space-3);
    margin-top: var(--space-4);
  }

  label {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  input:focus {
    outline: none;
    border-color: var(--accent);
  }
</style>
