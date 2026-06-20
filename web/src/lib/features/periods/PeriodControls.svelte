<script lang="ts">
  import { Tabs } from "bits-ui";
  import { periodFromPreset, todayIso, type PeriodPreset } from "$lib/date";
  import DatePickerField from "$lib/ui/DatePickerField.svelte";
  import { PERIOD_PRESETS } from "$lib/features/periods/presets";

  type Props = {
    preset: PeriodPreset;
    start: string;
    end: string;
    onPeriodChange: (value: { preset: PeriodPreset; start: string; end: string }) => void;
  };

  let { preset, start, end, onPeriodChange }: Props = $props();

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
    const range = periodFromPreset("custom", { start: iso, end });
    onPeriodChange({ preset: "custom", start: range.start, end: range.end });
  }

  function changeEnd(iso: string) {
    const range = periodFromPreset("custom", { start, end: iso });
    onPeriodChange({ preset: "custom", start: range.start, end: range.end });
  }
</script>

<div class="period">
  <Tabs.Root class="period-tabs" value={preset} onValueChange={selectPreset}>
    <Tabs.List class="period-tabs__list" aria-label="Period preset">
      {#each PERIOD_PRESETS as item (item.value)}
        <Tabs.Trigger class="period-tabs__trigger" value={item.value}>{item.label}</Tabs.Trigger>
      {/each}
    </Tabs.List>
  </Tabs.Root>

  {#if preset === "custom"}
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

  :global(.period-tabs__list) {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1px;
    border: 1px solid var(--border-strong);
    background: var(--border);
  }

  :global(.period-tabs__trigger) {
    min-height: 38px;
    border: 0;
    background: var(--panel);
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  :global(.period-tabs__trigger[data-state="active"]) {
    background: var(--surface);
    color: var(--accent);
  }

  .period__custom {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
  }

  @media (max-width: 520px) {
    .period__custom {
      grid-template-columns: 1fr;
    }
  }
</style>
