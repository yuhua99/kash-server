<script lang="ts">
  import { handleApiError } from "$lib/api/errors";
  import { validateCategoryName } from "$lib/validation";
  import Block from "$lib/ui/Block.svelte";
  import Button from "$lib/ui/Button.svelte";
  import FormField from "$lib/ui/FormField.svelte";
  import SegmentedControl from "$lib/ui/SegmentedControl.svelte";
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
    <FormField id="category-name" label="Name" {error}>
      <input id="category-name" bind:value={name} oninput={() => (error = "")} disabled={pending} autocomplete="off" />
    </FormField>

    <SegmentedControl
      items={[{ value: "expense", label: "Expense" }, { value: "income", label: "Income" }]}
      value={isIncome ? "income" : "expense"}
      onValueChange={(value) => {
        isIncome = value === "income";
      }}
      ariaLabel="Category type"
      disabled={pending}
    />

    <Button type="submit" disabled={pending}>{pending ? "Creating" : "Create category"}</Button>
  </form>
</Block>

<style>
  .category-form {
    display: grid;
    gap: var(--space-3);
  }
</style>
