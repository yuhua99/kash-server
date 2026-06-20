import type { PeriodPreset } from "$lib/date";

export const PERIOD_PRESETS: { value: PeriodPreset; label: string }[] = [
  { value: "month", label: "Month" },
  { value: "year", label: "Year" },
  { value: "custom", label: "Custom" },
];
