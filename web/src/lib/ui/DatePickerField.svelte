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
  const display = $derived.by(() => {
    const [y, m, d] = value.split("-");
    return y && m && d ? `${m}/${d}/${y}` : "";
  });
</script>

<div class="kash-dp">
  <DatePicker.Root
    value={dateValue}
    onValueChange={(v) => {
      if (v) onChange(dateValueToIso(v));
    }}
    {maxValue}
    {disabled}
    weekdayFormat="short"
    fixedWeeks={true}
  >
    <label for={id} class="kash-dp-label">{label}</label>
    <DatePicker.Trigger {id} class="kash-dp-trigger">
      <span class="kash-dp-value" class:kash-dp-placeholder={!value}>
        {display || "mm/dd/yyyy"}
      </span>
    </DatePicker.Trigger>
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

  :global(.kash-dp-trigger) {
    display: flex;
    align-items: center;
    min-height: 42px;
    padding: 0 var(--space-3);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 0.8125rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    text-align: left;
    cursor: pointer;
  }

  :global(.kash-dp-trigger:not(:disabled):hover),
  :global(.kash-dp-trigger:focus-visible) {
    border-color: var(--accent);
  }

  :global(.kash-dp-trigger:focus-visible),
  :global(.kash-dp-nav:focus-visible),
  :global(.kash-dp-day:focus-visible) {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }

  :global(.kash-dp-trigger:disabled) {
    color: var(--text-dim);
    cursor: not-allowed;
  }

  :global(.kash-dp-value) {
    font-variant-numeric: tabular-nums;
  }

  :global(.kash-dp-placeholder) {
    color: var(--text-dim);
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
