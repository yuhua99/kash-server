<script lang="ts">
  import { DatePicker } from "bits-ui";
  import { dateValueToIso, isoToDateValue } from "$lib/date";

  type Props = {
    id: string;
    label: string;
    value: string;
    onChange: (iso: string) => void;
    disabled?: boolean;
    maxIso?: string;
  };

  let { id, label, value, onChange, disabled = false, maxIso }: Props = $props();

  const dateValue = $derived(isoToDateValue(value));
  const maxValue = $derived(maxIso ? isoToDateValue(maxIso) : undefined);
</script>

<div class="kash-dp">
  <DatePicker.Root
    value={dateValue}
    onValueChange={(v) => onChange(dateValueToIso(v))}
    {maxValue}
    {disabled}
    weekdayFormat="short"
    fixedWeeks={true}
  >
    <DatePicker.Label class="kash-dp-label">{label}</DatePicker.Label>
  <div class="kash-dp-field">
    <DatePicker.Input {id} class="kash-dp-input">
      {#snippet children({ segments })}
        {#each segments as seg, i (i)}
          {#if seg.part === "literal"}
            <span class="kash-dp-lit">{seg.value}</span>
          {:else}
            <DatePicker.Segment part={seg.part} class="kash-dp-seg">{seg.value}</DatePicker.Segment>
          {/if}
        {/each}
      {/snippet}
    </DatePicker.Input>
    <DatePicker.Trigger class="kash-dp-trigger" aria-label="Open calendar">▣</DatePicker.Trigger>
  </div>
  <DatePicker.Content class="kash-dp-content">
    <DatePicker.Calendar class="kash-dp-cal">
      {#snippet children({ months, weekdays })}
        <DatePicker.Header class="kash-dp-header">
          <DatePicker.PrevButton class="kash-dp-nav">‹</DatePicker.PrevButton>
          <DatePicker.Heading class="kash-dp-heading" />
          <DatePicker.NextButton class="kash-dp-nav">›</DatePicker.NextButton>
        </DatePicker.Header>
        {#each months as month (month.value)}
          <DatePicker.Grid class="kash-dp-grid">
            <DatePicker.GridHead>
              <DatePicker.GridRow class="kash-dp-row">
                {#each weekdays as day (day)}
                  <DatePicker.HeadCell class="kash-dp-headcell">{day}</DatePicker.HeadCell>
                {/each}
              </DatePicker.GridRow>
            </DatePicker.GridHead>
            <DatePicker.GridBody>
              {#each month.weeks as weekDates, wi (wi)}
                <DatePicker.GridRow class="kash-dp-row">
                  {#each weekDates as date (date.toString())}
                    <DatePicker.Cell {date} month={month.value} class="kash-dp-cell">
                      <DatePicker.Day class="kash-dp-day">{date.day}</DatePicker.Day>
                    </DatePicker.Cell>
                  {/each}
                </DatePicker.GridRow>
              {/each}
            </DatePicker.GridBody>
          </DatePicker.Grid>
        {/each}
      {/snippet}
    </DatePicker.Calendar>
  </DatePicker.Content>
  </DatePicker.Root>
</div>

<style>
  :global(.kash-dp) {
    display: grid;
    gap: var(--space-2);
  }

  :global(.kash-dp-label) {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  :global(.kash-dp-field) {
    display: grid;
    grid-template-columns: 1fr auto;
    border: 1px solid var(--border);
    background: var(--surface);
  }

  :global(.kash-dp-field:focus-within) {
    border-color: var(--accent);
  }

  :global(.kash-dp-input) {
    display: flex;
    align-items: center;
    min-height: 42px;
    padding: 0 var(--space-3);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 0.8125rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  :global(.kash-dp-input:focus-visible),
  :global(.kash-dp-seg:focus-visible),
  :global(.kash-dp-trigger:focus-visible),
  :global(.kash-dp-nav:focus-visible),
  :global(.kash-dp-day:focus-visible) {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }

  :global(.kash-dp-lit) {
    color: var(--text-dim);
  }

  :global(.kash-dp-seg) {
    color: var(--text);
    padding: 0 1px;
    font-variant-numeric: tabular-nums;
  }

  :global(.kash-dp-seg[data-placeholder]) {
    color: var(--text-dim);
  }

  :global(.kash-dp-trigger) {
    width: 42px;
    border: 0;
    border-left: 1px solid var(--border);
    background: var(--surface);
    color: var(--accent);
    font-family: var(--font-mono);
    font-size: 0.875rem;
  }

  :global(.kash-dp-trigger:not(:disabled):hover) {
    border-color: var(--accent);
    color: var(--accent);
  }

  :global(.kash-dp-trigger:disabled) {
    color: var(--text-dim);
    cursor: not-allowed;
  }

  :global(.kash-dp-content) {
    z-index: 60;
    border: 1px solid var(--border-strong);
    background: var(--panel);
    padding: var(--space-3);
  }

  :global(.kash-dp-cal) {
    display: grid;
    gap: var(--space-3);
  }

  :global(.kash-dp-header) {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--space-2);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  :global(.kash-dp-heading) {
    text-align: center;
  }

  :global(.kash-dp-nav) {
    width: 32px;
    height: 32px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--accent);
    font-family: var(--font-mono);
    font-size: 1rem;
  }

  :global(.kash-dp-nav:not(:disabled):hover) {
    border-color: var(--accent);
    color: var(--accent);
  }

  :global(.kash-dp-nav:disabled) {
    color: var(--text-dim);
    cursor: not-allowed;
  }

  :global(.kash-dp-grid) {
    border-collapse: collapse;
    border-spacing: 0;
  }

  :global(.kash-dp-row) {
    display: grid;
    grid-template-columns: repeat(7, 32px);
  }

  :global(.kash-dp-headcell) {
    display: grid;
    place-items: center;
    width: 32px;
    height: 24px;
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  :global(.kash-dp-cell) {
    width: 32px;
    height: 32px;
    padding: 0;
  }

  :global(.kash-dp-day) {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 0.75rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  :global(.kash-dp-day:not([data-disabled]):not([data-unavailable]):hover) {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--bg);
  }

  :global(.kash-dp-day[data-outside-month]) {
    color: var(--text-dim);
  }

  :global(.kash-dp-day[data-disabled]),
  :global(.kash-dp-day[data-unavailable]) {
    color: var(--text-dim);
    cursor: not-allowed;
  }

  :global(.kash-dp-day[data-today]) {
    outline: 1px solid var(--border-strong);
    outline-offset: -2px;
  }

  :global(.kash-dp-day[data-selected]) {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--bg);
  }
</style>
