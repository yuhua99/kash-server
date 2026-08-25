<script lang="ts">
  import { handleApiError } from "$lib/api/errors";
  import type { components } from "$lib/api/schema";
  import Button from "$lib/ui/Button.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import ListRow from "$lib/ui/ListRow.svelte";
  import { toast } from "$lib/ui/toast";
  import { deleteCategory } from "$lib/features/categories/api";
  import { invalidateCategoriesCache } from "$lib/features/categories/cache";
  import CategoryEditDialog from "$lib/features/categories/CategoryEditDialog.svelte";

  type Category = components["schemas"]["Category"];

  type Props = {
    categories: Category[];
    onChange?: () => void;
  };

  let { categories, onChange }: Props = $props();

  let editingCategory = $state<Category | null>(null);
  let editOpen = $state(false);
  let pendingDeleteCategory = $state<Category | null>(null);
  let deleteOpen = $state(false);
  let deleting = $state(false);

  function openEdit(category: Category) {
    editingCategory = category;
    editOpen = true;
  }

  function closeEdit() {
    editOpen = false;
    editingCategory = null;
  }

  function openDelete(category: Category) {
    pendingDeleteCategory = category;
    deleteOpen = true;
  }

  function closeDelete() {
    deleteOpen = false;
    pendingDeleteCategory = null;
  }

  async function confirmDelete() {
    if (!pendingDeleteCategory) {
      return;
    }

    deleting = true;

    try {
      await deleteCategory(pendingDeleteCategory.id);
      invalidateCategoriesCache();
      toast.success("Category deleted");
      onChange?.();
      closeDelete();
    } catch (e) {
      const m = await handleApiError(e, "Could not delete category");
      if (m) {
        toast.error(m);
      }
    } finally {
      deleting = false;
    }
  }

  function handleSaved() {
    onChange?.();
    closeEdit();
  }
</script>

<div class="category-list">
  {#if categories.length === 0}
    <EmptyState message="No categories found." />
  {:else}
    {#each categories as category (category.id)}
      <ListRow>
        <div class="row">
          <div class="row__main">
            <span class="row__name">{category.name}</span>
            <samp class:tag--income={category.is_income} class="tag">
              {category.is_income ? "INCOME" : "EXPENSE"}
            </samp>
          </div>
          <div class="row__actions">
            <Button variant="secondary" size="compact" onclick={() => openEdit(category)}>Edit</Button>
            <Button
              variant="danger"
              size="compact"
              onclick={() => openDelete(category)}
            >
              Delete
            </Button>
          </div>
        </div>
      </ListRow>
    {/each}
  {/if}
</div>

<CategoryEditDialog bind:open={editOpen} category={editingCategory} onOpenChange={(open) => !open && closeEdit()} onSaved={handleSaved} />
<ConfirmDialog
  bind:open={deleteOpen}
  onOpenChange={(open) => !open && closeDelete()}
  title="Delete category"
  description={pendingDeleteCategory ? `Delete ${pendingDeleteCategory.name}?` : "Delete this category?"}
  confirmLabel="Delete"
  confirmBusyLabel="Deleting"
  busy={deleting}
  onConfirm={confirmDelete}
/>

<style>
  .category-list {
    border: 1px solid var(--border);
  }

  .row {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: var(--space-3);
  }

  .row__main {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .row__name {
    overflow: hidden;
    color: var(--text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    font-weight: 600;
    letter-spacing: 0.1em;
  }

  .tag--income {
    color: var(--success);
  }

  .row__actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-2);
  }

  @media (max-width: 520px) {
    .row {
      grid-template-columns: 1fr;
    }

    .row__actions {
      justify-content: flex-start;
    }
  }
</style>
