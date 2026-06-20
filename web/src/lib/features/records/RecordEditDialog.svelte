<script lang="ts">
  import { handleApiError } from "$lib/api/errors";
  import type { components } from "$lib/api/schema";
  import { validateAmount, validateDate, validateRecordName } from "$lib/validation";
  import Button from "$lib/ui/Button.svelte";
  import Dialog from "$lib/ui/Dialog.svelte";
  import DatePickerField from "$lib/ui/DatePickerField.svelte";
  import SelectField from "$lib/ui/SelectField.svelte";
  import { toast } from "$lib/ui/toast";
  import { amountDisplayMode, formatAmount } from "$lib/features/money/amount-display";
  import { updateRecord } from "$lib/features/records/api";
  import { invalidateRecordsCache } from "$lib/features/records/cache";

  type RecordItem = components["schemas"]["Record"];
  type Category = components["schemas"]["Category"];

  type Props = {
    open: boolean;
    onOpenChange?: (open: boolean) => void;
    record: RecordItem | null;
    categories: Category[];
    onSaved?: () => void;
  };

  let { open = $bindable(false), onOpenChange, record, categories, onSaved }: Props = $props();

  let name = $state("");
  let amountInput = $state("");
  let isIncome = $state(false);
  let categoryId = $state("");
  let date = $state("");
  let error = $state("");
  let pending = $state(false);
  let previousOpen = $state(false);
  $effect(() => {
    if (open && !previousOpen && record) {
      name = record.name;
      isIncome = record.amount > 0;
      amountInput = formatAmount(Math.abs(record.amount), $amountDisplayMode);
      categoryId = record.category_id ?? "";
      date = record.date;
      error = "";
    }
    previousOpen = open;
  });

  const categoryItems = $derived(
    categories
      .filter((category) => category.is_income === isIncome)
      .map((category) => ({ value: category.id, label: category.name })),
  );

  async function save() {
    if (!record) {
      return;
    }

    const trimmedName = name.trim();
    const nameError = validateRecordName(trimmedName);
    if (nameError) {
      error = nameError;
      return;
    }

    const amount = Number(amountInput);
    if (amount < 0) {
      error = "Amount cannot be negative.";
      return;
    }
    const amountError = validateAmount(amount);
    if (amountError) {
      error = amountError;
      return;
    }

    const dateError = validateDate(date);
    if (dateError) {
      error = dateError;
      return;
    }

    error = "";
    pending = true;

    try {
      await updateRecord(record.id, {
        name: trimmedName,
        amount: isIncome ? amount : -amount,
        category_id: categoryId || null,
        date,
      });
      invalidateRecordsCache();
      toast.success("Record updated");
      onSaved?.();
      open = false;
      onOpenChange?.(false);
    } catch (e) {
      const message = await handleApiError(e, "Could not update record");
      if (message) {
        toast.error(message);
      }
    } finally {
      pending = false;
    }
  }
</script>

<Dialog bind:open {onOpenChange} title="Edit record">
  <form class="edit-form" onsubmit={(event) => event.preventDefault()}>
    <div class="field">
      <label for="edit-record-name">Name</label>
      <input id="edit-record-name" bind:value={name} autocomplete="off" />
    </div>

    <div class="field">
      <label for="edit-record-amount">Amount</label>
      <input
        id="edit-record-amount"
        type="number"
        min="0"
        step={$amountDisplayMode === "whole" ? "1" : "0.01"}
        bind:value={amountInput}
      />
    </div>

    <div class="toggle">
      <Button
        variant={isIncome ? "secondary" : "primary"}
        size="compact"
        onclick={() => (isIncome = false)}
      >
        Expense
      </Button>
      <Button
        variant={isIncome ? "primary" : "secondary"}
        size="compact"
        onclick={() => (isIncome = true)}
      >
        Income
      </Button>
    </div>

    <SelectField
      id="edit-record-category"
      label="Category"
      value={categoryId}
      items={categoryItems}
      onValueChange={(value) => (categoryId = value)}
    />

    <DatePickerField
      id="edit-record-date"
      label="Date"
      value={date}
      onChange={(iso) => (date = iso)}
    />

    {#if error}
      <p role="alert">{error}</p>
    {/if}

    <Button variant="primary" disabled={pending} onclick={save}>
      {pending ? "Saving" : "Save"}
    </Button>
  </form>
</Dialog>

<style>
  .edit-form {
    display: grid;
    gap: var(--space-3);
    margin-top: var(--space-4);
  }

  .field {
    display: grid;
    gap: var(--space-2);
  }

  .toggle {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-2);
  }
</style>
