<script lang="ts">
  import { periodFromPreset, todayIso, type PeriodPreset } from "$lib/date";
  import DatePickerField from "$lib/ui/DatePickerField.svelte";
  import SelectField from "$lib/ui/SelectField.svelte";
  import SegmentedControl from "$lib/ui/SegmentedControl.svelte";
  import { PERIOD_PRESETS } from "$lib/features/periods/presets";

  const MONTH_NAMES = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
  ];
  const YEARS_BACK = 10;
  const today = todayIso();
  const currentYear = Number(today.slice(0, 4));
  const currentMonth = Number(today.slice(5, 7));
  const yearItems = Array.from({ length: YEARS_BACK + 1 }, (_, i) => {
    const year = String(currentYear - i);
    return { value: year, label: year };
  });

  type Props = {
    preset: PeriodPreset;
    start: string;
    end: string;
    onPeriodChange: (value: { preset: PeriodPreset; start: string; end: string }) => void;
  };

  let { preset, start, end, onPeriodChange }: Props = $props();

  const selYear = $derived(Number(start.slice(0, 4)) || currentYear);
  const selMonth = $derived(Number(start.slice(5, 7)) || currentMonth);
  const monthItems = $derived.by(() => {
    const maxMonth = selYear === currentYear ? currentMonth : 12;
    return MONTH_NAMES.slice(0, maxMonth).map((label, i) => ({ value: String(i + 1), label }));
  });

  function selectYear(value: string) {
    const year = Number(value);
    if (preset === "year") {
      const range = periodFromPreset("year", { year });
      onPeriodChange({ preset: "year", start: range.start, end: range.end });
      return;
    }
    const maxMonth = year === currentYear ? currentMonth : 12;
    const month = Math.min(selMonth, maxMonth);
    const range = periodFromPreset("month", { year, month });
    onPeriodChange({ preset: "month", start: range.start, end: range.end });
  }

  function selectMonth(value: string) {
    const range = periodFromPreset("month", { year: selYear, month: Number(value) });
    onPeriodChange({ preset: "month", start: range.start, end: range.end });
  }

  function selectPreset(value: string) {
    const next = value as PeriodPreset;
    if (next === "custom") {
      onPeriodChange({ preset: "custom", start, end });
      return;
    }
    const range = periodFromPreset(next);
    onPeriodChange({ preset: next, start: range.start, end: range.end });
  }

  function changeStart(iso: string) {
    const clampedEnd = iso > end ? iso : end;
    const range = periodFromPreset("custom", { start: iso, end: clampedEnd });
    if (range.start > range.end) range.start = range.end;
    onPeriodChange({ preset: "custom", start: range.start, end: range.end });
  }

  function changeEnd(iso: string) {
    const clampedStart = iso < start ? iso : start;
    const range = periodFromPreset("custom", { start: clampedStart, end: iso });
    if (range.start > range.end) range.start = range.end;
    onPeriodChange({ preset: "custom", start: range.start, end: range.end });
  }
</script>

<div class="period">
  <SegmentedControl
    items={PERIOD_PRESETS}
    value={preset}
    onValueChange={selectPreset}
    ariaLabel="Period preset"
  />

  {#if preset === "month"}
    <div class="period__selects">
      <SelectField
        id="period-year"
        label="Year"
        value={String(selYear)}
        items={yearItems}
        onValueChange={selectYear}
      />
      <SelectField
        id="period-month"
        label="Month"
        value={String(selMonth)}
        items={monthItems}
        onValueChange={selectMonth}
      />
    </div>
  {:else if preset === "year"}
    <div class="period__selects period__selects--single">
      <SelectField
        id="period-year"
        label="Year"
        value={String(selYear)}
        items={yearItems}
        onValueChange={selectYear}
      />
    </div>
  {:else}
    <div class="period__custom">
      <DatePickerField
        id="period-start"
        label="Start"
        value={start}
        maxIso={end || todayIso()}
        onChange={changeStart}
      />
      <DatePickerField id="period-end" label="End" value={end} maxIso={todayIso()} onChange={changeEnd} />
    </div>
  {/if}
</div>

<style>
  .period {
    display: grid;
    gap: var(--space-3);
  }

  .period__custom,
  .period__selects {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
  }

  .period__selects--single {
    grid-template-columns: 1fr;
  }

  @media (max-width: 520px) {
    .period__custom,
    .period__selects {
      grid-template-columns: 1fr;
    }
  }
</style>
