<script lang="ts">
  import { Collapsible } from "bits-ui";
  import { onMount } from "svelte";
  import { handleApiError } from "$lib/api/errors";
  import type { components } from "$lib/api/schema";
  import { todayIso } from "$lib/date";
  import { validateAmount, validateDate, validateRecordName } from "$lib/validation";
  import Button from "$lib/ui/Button.svelte";
  import Chip from "$lib/ui/Chip.svelte";
  import DatePickerField from "$lib/ui/DatePickerField.svelte";
  import FormField from "$lib/ui/FormField.svelte";
  import SegmentedControl from "$lib/ui/SegmentedControl.svelte";
  import SelectField from "$lib/ui/SelectField.svelte";
  import StatusMessage from "$lib/ui/StatusMessage.svelte";
  import { toast } from "$lib/ui/toast";
  import {
    amountDisplayMode,
    amountInputStep,
    normalizeAmountInputValue,
    type AmountDisplayMode,
  } from "$lib/features/money/amount-display";
  import { currentCurrency } from "$lib/features/money/current-currency";
  import { getCategoriesCached } from "$lib/features/categories/cache";
  import { getAcceptedFriendsCached } from "$lib/features/friends/cache";
  import VoiceEntry from "./VoiceEntry.svelte";
  import { voiceDraftFields } from "./voice-draft";
  import type { SplitPrefill } from "$lib/features/splits/allocation";
  import { createRecord } from "$lib/features/records/api";
  import { getRecentRecordsCached, invalidateRecordsCache } from "$lib/features/records/cache";
  import { createSplit } from "$lib/features/splits/api";
  import { generateIdempotencyKey } from "$lib/features/splits/idempotency";
  import SplitEditor, { type SplitResult } from "$lib/features/splits/SplitEditor.svelte";

  type Category = components["schemas"]["Category"];
  type FriendshipRelation = components["schemas"]["FriendshipRelation"];
  type RecordItem = components["schemas"]["Record"];

  let categories = $state<Category[]>([]);
  let friends = $state<FriendshipRelation[]>([]);
  let recentRecords = $state<RecordItem[]>([]);

  let name = $state("");
  let amountInput = $state("");
  let isIncome = $state(false);
  let categoryId = $state("");
  let date = $state(todayIso());
  let error = $state("");
  let pending = $state(false);
  let voiceBusy = $state(false);
  let voiceContext: { currency: string; mode: AmountDisplayMode } | undefined;

  let splitEnabled = $state(false);
  let splitPrefill = $state<SplitPrefill>();
  let splitResult = $state<SplitResult>({ participants: [], valid: false, error: null });
  let idempotencyKey = generateIdempotencyKey();

  onMount(async () => {
    const [cats, frs, recents] = await Promise.all([
      getCategoriesCached().catch(() => [] as Category[]),
      getAcceptedFriendsCached().catch(() => [] as FriendshipRelation[]),
      getRecentRecordsCached().catch(() => [] as RecordItem[]),
    ]);
    categories = cats;
    friends = frs;
    recentRecords = recents;
  });

  const categoryItems = $derived(
    categories
      .filter((category) => category.is_income === isIncome)
      .map((category) => ({ value: category.id, label: category.name })),
  );

  const total = $derived(
    normalizeAmountInputValue(Number(amountInput) || 0, $amountDisplayMode, $currentCurrency),
  );

  const suggestions = $derived.by(() => {
    if (!categoryId) {
      return [] as string[];
    }
    const ranked = recentRecords
      .filter((record) => record.category_id === categoryId)
      .slice()
      .sort((a, b) => Math.abs(Math.abs(a.amount) - total) - Math.abs(Math.abs(b.amount) - total));
    const seen = new Set<string>();
    const names: string[] = [];
    for (const record of ranked) {
      if (!seen.has(record.name)) {
        seen.add(record.name);
        names.push(record.name);
      }
      if (names.length >= 5) {
        break;
      }
    }
    return names;
  });

  function setVoiceBusy(busy: boolean) {
    if (busy && !voiceBusy) {
      voiceContext = { currency: $currentCurrency, mode: $amountDisplayMode };
    }
    voiceBusy = busy;
  }

  function applyVoiceDraft(draft: components["schemas"]["VoiceEntryResponse"]) {
    if (voiceContext?.currency !== $currentCurrency || voiceContext?.mode !== $amountDisplayMode) {
      throw new Error("Currency or amount display mode changed. Please record again.");
    }
    const fields = voiceDraftFields(draft, categories, friends, $amountDisplayMode, $currentCurrency);
    name = fields.name;
    amountInput = fields.amountInput;
    isIncome = fields.isIncome;
    categoryId = fields.categoryId;
    date = fields.date;
    splitPrefill = fields.splitPrefill;
    splitEnabled = fields.splitEnabled;
    error = "";
    idempotencyKey = generateIdempotencyKey();
  }

  function selectType(income: boolean) {
    isIncome = income;
    categoryId = "";
  }

  function resetForm() {
    name = "";
    amountInput = "";
    categoryId = "";
    date = todayIso();
    splitEnabled = false;
    splitPrefill = undefined;
    idempotencyKey = generateIdempotencyKey();
  }

  function validateBase(): string | null {
    const trimmedName = name.trim();
    const nameError = validateRecordName(trimmedName);
    if (nameError) {
      return nameError;
    }
    if (total < 0) {
      return "Amount cannot be negative.";
    }
    const amountError = validateAmount(total);
    if (amountError) {
      return amountError;
    }
    if (!categoryId) {
      return "Select a category.";
    }
    return validateDate(date);
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (pending || voiceBusy) return;
    const baseError = validateBase();
    if (baseError) {
      error = baseError;
      return;
    }
    error = "";

    if (splitEnabled) {
      await submitSplit();
      return;
    }

    pending = true;
    try {
      await createRecord({
        name: name.trim(),
        amount: isIncome ? total : -total,
        currency: $currentCurrency,
        category_id: categoryId,
        date,
      });
      invalidateRecordsCache();
      toast.success("Record added");
      resetForm();
    } catch (e) {
      const message = await handleApiError(e, "Could not add record");
      if (message) {
        toast.error(message);
      }
    } finally {
      pending = false;
    }
  }

  async function submitSplit() {
    if (!splitResult.valid) {
      error = splitResult.error ?? "Invalid split.";
      return;
    }

    pending = true;
    try {
      await createSplit({
        idempotency_key: idempotencyKey,
        total_amount: total,
        currency: $currentCurrency,
        description: name.trim(),
        date,
        category_id: categoryId,
        splits: splitResult.participants,
      });
      invalidateRecordsCache();
      toast.success("Split created");
      resetForm();
    } catch (e) {
      if ((e as { status?: number }).status === 409) {
        idempotencyKey = generateIdempotencyKey();
        toast.error("Duplicate key conflict — please try again.");
        return;
      }
      const message = await handleApiError(e, "Could not create split");
      if (message) {
        toast.error(message);
      }
    } finally {
      pending = false;
    }
  }
</script>

<form class="quick-add" onsubmit={submit}>
  <fieldset disabled={pending || voiceBusy}>
  <FormField id="quick-amount" label="Amount ({$currentCurrency})">
    <input
      id="quick-amount"
      type="number"
      min="0"
      step={amountInputStep($amountDisplayMode, $currentCurrency)}
      bind:value={amountInput}
    />
  </FormField>

  <SegmentedControl
    items={[{ value: "expense", label: "Expense" }, { value: "income", label: "Income" }]}
    value={isIncome ? "income" : "expense"}
    onValueChange={(value) => selectType(value === "income")}
    ariaLabel="Record type"
  />

  <SelectField
    id="quick-category"
    label="Category"
    value={categoryId}
    items={categoryItems}
    onValueChange={(value) => (categoryId = value)}
  />

  <FormField id="quick-name" label="Name">
    <input id="quick-name" bind:value={name} autocomplete="off" />
    {#if suggestions.length > 0}
      <div class="suggestions">
        {#each suggestions as suggestion (suggestion)}
          <Chip onclick={() => (name = suggestion)}>{suggestion}</Chip>
        {/each}
      </div>
    {/if}
  </FormField>

  <DatePickerField id="quick-date" label="Date" value={date} maxIso={todayIso()} onChange={(iso) => (date = iso)} />

  <Collapsible.Root bind:open={splitEnabled}>
    <Collapsible.Trigger class="split-toggle">{splitEnabled ? "Disable split" : "Split with friends"}</Collapsible.Trigger>
    <Collapsible.Content class="split-content">
      {#if splitEnabled}
        <SplitEditor {friends} {total} mode={$amountDisplayMode} currency={$currentCurrency} prefill={splitPrefill} bind:result={splitResult} />
      {/if}
    </Collapsible.Content>
  </Collapsible.Root>
  </fieldset>

  {#if error}
    <StatusMessage kind="error" message={error} />
  {/if}

  <VoiceEntry
    disabled={pending || categories.length === 0}
    currency={$currentCurrency}
    onBusyChange={setVoiceBusy}
    onDraft={applyVoiceDraft}
  />
  <Button variant="primary" type="submit" disabled={voiceBusy} busy={pending} busyLabel="Saving">
    {splitEnabled ? "Create split" : "Add record"}
  </Button>
</form>

<style>
  .quick-add {
    display: grid;
    gap: var(--space-4);
  }

  fieldset {
    display: grid;
    gap: var(--space-4);
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .suggestions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  :global(.split-toggle) {
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border-strong);
    background: var(--panel);
    color: var(--accent);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  :global(.split-content) {
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-top: 0;
  }
</style>
