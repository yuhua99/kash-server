<script lang="ts">
  import { onMount } from "svelte";
  import type { components } from "$lib/api/schema";
  import { handleApiError } from "$lib/api/errors";
  import { periodFromPreset, type PeriodPreset } from "$lib/date";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import { toast } from "$lib/ui/toast";
  import { getCategoriesCached } from "$lib/features/categories/cache";
  import { getFxRates } from "$lib/features/fx/api";
  import { buildRateLookup, convertAmountToMainCurrency } from "$lib/features/money/fx";
  import { deleteRecord } from "$lib/features/records/api";
  import { invalidateRecordsCache } from "$lib/features/records/cache";
  import { getAllRecordsByDateRange } from "$lib/features/records/query";
  import RecordFilters from "$lib/features/records/RecordFilters.svelte";
  import RecordList from "$lib/features/records/RecordList.svelte";
  import RecordEditDialog from "$lib/features/records/RecordEditDialog.svelte";
  import {
    compareRecords,
    groupRecordsByDate,
    matchesRecordFilters,
    type CategoryFilterMode,
    type SortMode,
  } from "$lib/features/records/view";
  import { listPendingShares } from "$lib/features/splits/api";
  import PendingSharesSection from "$lib/features/splits/PendingSharesSection.svelte";

  type Category = components["schemas"]["Category"];
  type RecordItem = components["schemas"]["Record"];
  type PendingShare = components["schemas"]["PendingShare"];

  let { data } = $props();
  const mainCurrency = $derived(data.mainCurrency as string);

  const initial = periodFromPreset("month");
  let preset = $state<PeriodPreset>("month");
  let start = $state(initial.start);
  let end = $state(initial.end);
  let search = $state("");
  let categoryFilter = $state<CategoryFilterMode>("all_expenses");
  let sortMode = $state<SortMode>("date_desc");

  let categories = $state<Category[]>([]);
  let records = $state<RecordItem[]>([]);
  let pendingShares = $state<PendingShare[]>([]);
  let loading = $state(true);
  let error = $state("");

  let convertedById = $state<Map<string, number>>(new Map());
  let convertedSpendById = $state<Map<string, number>>(new Map());
  let displayCurrency = $state("");
  let convertSeq = 0;

  let editRecord = $state<RecordItem | null>(null);
  let editOpen = $state(false);
  let pendingDelete = $state<RecordItem | null>(null);
  let deleteOpen = $state(false);
  let deleting = $state(false);

  const categoryNames = $derived(new Map(categories.map((c) => [c.id, c.name])));
  const grouped = $derived(sortMode === "date_desc" || sortMode === "date_asc");

  const visibleRecords = $derived.by(() => {
    const normalizedSearch = search.trim().toLowerCase();
    return records
      .filter((record) => matchesRecordFilters(record, { normalizedSearch, categoryFilter }))
      .slice()
      .sort((a, b) => compareRecords(a, b, sortMode, convertedById));
  });

  const groups = $derived(
    grouped ? groupRecordsByDate(visibleRecords, sortMode, convertedSpendById, displayCurrency) : [],
  );

  async function convert(list: RecordItem[]) {
    const seq = ++convertSeq;
    if (!mainCurrency) {
      convertedById = new Map();
      convertedSpendById = new Map();
      displayCurrency = "";
      return;
    }

    const quotes = new Set(list.map((record) => record.currency));
    quotes.add(mainCurrency);

    try {
      const response = await getFxRates({ from: start, to: end, quotes: [...quotes] });
      if (seq !== convertSeq) {
        return;
      }
      const lookup = buildRateLookup(response.rates);
      const byId = new Map<string, number>();
      const spendById = new Map<string, number>();
      for (const record of list) {
        const value = convertAmountToMainCurrency(
          record.amount,
          record.currency,
          mainCurrency,
          record.date,
          lookup,
        );
        byId.set(record.id, value);
        if (record.amount < 0) {
          spendById.set(record.id, Math.abs(value));
        }
      }
      if (seq !== convertSeq) {
        return;
      }
      convertedById = byId;
      convertedSpendById = spendById;
      displayCurrency = mainCurrency;
    } catch {
      if (seq !== convertSeq) {
        return;
      }
      convertedById = new Map();
      convertedSpendById = new Map();
      displayCurrency = "";
    }
  }

  async function loadData() {
    loading = true;
    error = "";
    try {
      const [recs, shares] = await Promise.all([
        getAllRecordsByDateRange({ startDate: start, endDate: end }),
        listPendingShares({ limit: 1000 }).catch(() => [] as PendingShare[]),
      ]);
      records = recs;
      pendingShares = shares;
      await convert(recs);
    } catch (e) {
      const message = await handleApiError(e, "Could not load records");
      if (message) {
        error = message;
      }
    } finally {
      loading = false;
    }
  }

  function changePeriod(value: { preset: PeriodPreset; start: string; end: string }) {
    preset = value.preset;
    start = value.start;
    end = value.end;
    void loadData();
  }

  function onEdit(record: RecordItem) {
    editRecord = record;
    editOpen = true;
  }

  function onDelete(record: RecordItem) {
    pendingDelete = record;
    deleteOpen = true;
  }

  async function confirmDelete() {
    if (!pendingDelete) {
      return;
    }
    deleting = true;
    try {
      await deleteRecord(pendingDelete.id);
      invalidateRecordsCache();
      toast.success("Record deleted");
      deleteOpen = false;
      pendingDelete = null;
      await loadData();
    } catch (e) {
      const message = await handleApiError(e, "Could not delete record");
      if (message) {
        toast.error(message);
      }
    } finally {
      deleting = false;
    }
  }

  onMount(async () => {
    categories = await getCategoriesCached().catch(() => [] as Category[]);
    await loadData();
  });
</script>

<section class="page">
  <h1>Records</h1>

  <PendingSharesSection shares={pendingShares} />

  <RecordFilters
    {preset}
    {start}
    {end}
    {search}
    {categoryFilter}
    {sortMode}
    {categories}
    onPeriodChange={changePeriod}
    onSearchChange={(value) => (search = value)}
    onCategoryFilterChange={(value) => (categoryFilter = value)}
    onSortChange={(value) => (sortMode = value)}
  />

  {#if loading}
    <p role="status">Loading…</p>
  {:else if error}
    <p role="alert">{error}</p>
  {:else}
    <RecordList {grouped} {groups} records={visibleRecords} {categoryNames} {onEdit} {onDelete} />
  {/if}
</section>

<RecordEditDialog
  bind:open={editOpen}
  record={editRecord}
  {categories}
  onOpenChange={(open) => !open && (editRecord = null)}
  onSaved={loadData}
/>
<ConfirmDialog
  bind:open={deleteOpen}
  onOpenChange={(open) => !open && (pendingDelete = null)}
  title="Delete record"
  description={pendingDelete ? `Delete ${pendingDelete.name}?` : "Delete this record?"}
  confirmLabel="Delete"
  confirmBusyLabel="Deleting"
  busy={deleting}
  onConfirm={confirmDelete}
/>

<style>
  .page {
    display: grid;
    gap: var(--space-4);
  }

  h1 {
    font-family: var(--font-display);
    font-size: clamp(2rem, 9vw, 3.5rem);
    font-weight: 900;
    letter-spacing: -0.04em;
    text-transform: uppercase;
  }
</style>
