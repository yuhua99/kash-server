<script lang="ts">
  import { onMount } from "svelte";
  import type { components } from "$lib/api/schema";
  import { handleApiError } from "$lib/api/errors";
  import { periodFromPreset, type PeriodPreset } from "$lib/date";
  import { getCategoriesCached } from "$lib/features/categories/cache";
  import { getFxRates } from "$lib/features/fx/api";
  import { buildRateLookup, convertAmountToMainCurrency } from "$lib/features/money/fx";
  import { getAllRecordsByDateRange } from "$lib/features/records/query";
  import PeriodControls from "$lib/features/periods/PeriodControls.svelte";
  import StatsBreakdown from "$lib/features/stats/StatsBreakdown.svelte";
  import { buildBreakdown, calculateTotals } from "$lib/features/stats/query";

  type Category = components["schemas"]["Category"];
  type RecordItem = components["schemas"]["Record"];

  let { data } = $props();
  const mainCurrency = $derived(data.mainCurrency as string);

  const initial = periodFromPreset("month");
  let preset = $state<PeriodPreset>("month");
  let start = $state(initial.start);
  let end = $state(initial.end);

  let categories = $state<Category[]>([]);
  let statsRecords = $state<RecordItem[]>([]);
  let statsCurrency = $state("TWD");
  let note = $state<string | null>(null);
  let loading = $state(true);
  let error = $state("");
  let seq = 0;

  const totals = $derived(calculateTotals(statsRecords));
  const breakdown = $derived(buildBreakdown(statsRecords, categories));

  async function loadData() {
    const current = ++seq;
    loading = true;
    error = "";
    try {
      const records = await getAllRecordsByDateRange({ startDate: start, endDate: end });
      if (current !== seq) {
        return;
      }

      if (!mainCurrency) {
        statsRecords = records;
        statsCurrency = records[0]?.currency ?? "TWD";
        note = "Set a main currency in settings for combined stats.";
        return;
      }

      const quotes = new Set(records.map((record) => record.currency));
      quotes.add(mainCurrency);
      const response = await getFxRates({ from: start, to: end, quotes: [...quotes] });
      if (current !== seq) {
        return;
      }
      const lookup = buildRateLookup(response.rates);
      statsRecords = records.map((record) => ({
        ...record,
        amount: convertAmountToMainCurrency(record.amount, record.currency, mainCurrency, record.date, lookup),
      }));
      statsCurrency = mainCurrency;
      note = null;
    } catch (e) {
      if (current !== seq) {
        return;
      }
      const message = await handleApiError(e, "Could not load stats");
      if (message) {
        error = message;
      }
    } finally {
      if (current === seq) {
        loading = false;
      }
    }
  }

  function changePeriod(value: { preset: PeriodPreset; start: string; end: string }) {
    preset = value.preset;
    start = value.start;
    end = value.end;
    void loadData();
  }

  onMount(async () => {
    categories = await getCategoriesCached().catch(() => [] as Category[]);
    await loadData();
  });
</script>

<section class="page">
  <h1>Stats</h1>

  <PeriodControls {preset} {start} {end} onPeriodChange={changePeriod} />

  {#if loading}
    <p role="status">Loading…</p>
  {:else if error}
    <p role="alert">{error}</p>
  {:else}
    <StatsBreakdown {totals} {breakdown} currency={statsCurrency} {note} />
  {/if}
</section>

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
