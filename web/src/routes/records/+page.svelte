<script lang="ts">
  import { onMount } from "svelte";
  import type { components } from "$lib/api/schema";
  import { handleApiError } from "$lib/api/errors";
  import { periodFromPreset, type PeriodPreset } from "$lib/date";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import { toast } from "$lib/ui/toast";
  import { getCategoriesCached } from "$lib/features/categories/cache";
  import { convertRecords } from "$lib/features/money/conversion";
  import { getSettingsCached } from "$lib/features/settings/cache";
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

  const RENDER_PAGE_SIZE = 100;

  let mainCurrency = $state("");

  const initial = periodFromPreset("month");
  let preset = $state<PeriodPreset>("month");
  let start = $state(initial.start);
  let end = $state(initial.end);
  let search = $state("");
  let debouncedSearch = $state("");
  let categoryFilter = $state<CategoryFilterMode>("all_expenses");
  let sortMode = $state<SortMode>("date_desc");
  let renderLimit = $state(RENDER_PAGE_SIZE);

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

  $effect(() => {
    if (!search) {
      debouncedSearch = "";
      return;
    }

    const timeout = setTimeout(() => {
      debouncedSearch = search;
    }, 200);

    return () => clearTimeout(timeout);
  });

  $effect(() => {
    void debouncedSearch;
    void categoryFilter;
    void sortMode;
    void start;
    void end;
    renderLimit = RENDER_PAGE_SIZE;
  });

  const visibleRecords = $derived.by(() => {
    const normalizedSearch = debouncedSearch.trim().toLowerCase();
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
    try {
      const result = await convertRecords(list, mainCurrency, { from: start, to: end });
      if (seq !== convertSeq) return;
      convertedById = result.convertedById;
      convertedSpendById = result.convertedSpendById;
      displayCurrency = result.displayCurrency;
    } catch (e) {
      if (seq !== convertSeq) return;
      const message = await handleApiError(e, "Could not load exchange rates");
      if (message) toast.error(message);
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
    const [cats, settings] = await Promise.all([
      getCategoriesCached().catch(() => [] as Category[]),
      getSettingsCached()
        .then((s) => s.main_currency)
        .catch(() => ""),
    ]);
    categories = cats;
    mainCurrency = settings;
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
    <RecordList
      {grouped}
      {groups}
      records={visibleRecords}
      {categoryNames}
      limit={renderLimit}
      onShowMore={() => (renderLimit += RENDER_PAGE_SIZE)}
      {onEdit}
      {onDelete}
    />
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
</style>
