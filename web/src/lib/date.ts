import { parseDate, type DateValue } from "@internationalized/date";

export type PeriodPreset = "month" | "year" | "custom";

type PeriodOptions = {
  year?: number;
  month?: number;
  start?: string;
  end?: string;
};

type PeriodRange = {
  start: string;
  end: string;
};

function pad(value: number): string {
  return String(value).padStart(2, "0");
}

function formatDate(year: number, month: number, day: number): string {
  return `${year}-${pad(month)}-${pad(day)}`;
}

export function todayIso(): string {
  const today = new Date();
  return formatDate(today.getFullYear(), today.getMonth() + 1, today.getDate());
}

export function isoToDateValue(value: string): DateValue | undefined {
  if (!value) {
    return undefined;
  }

  try {
    return parseDate(value);
  } catch {
    return undefined;
  }
}

export function dateValueToIso(value: DateValue | null | undefined): string {
  return value?.toString() ?? "";
}

export function periodFromPreset(preset: PeriodPreset, options?: PeriodOptions): PeriodRange {
  const today = new Date();
  const currentYear = today.getFullYear();
  const currentMonth = today.getMonth() + 1;
  let start = "";
  let end = "";

  if (preset === "month") {
    const year = options?.year ?? currentYear;
    const month = options?.month ?? currentMonth;
    const lastDay = new Date(year, month, 0).getDate();
    start = formatDate(year, month, 1);
    end = formatDate(year, month, lastDay);
  } else if (preset === "year") {
    const year = options?.year ?? currentYear;
    start = formatDate(year, 1, 1);
    end = formatDate(year, 12, 31);
  } else {
    start = options?.start ?? "";
    end = options?.end ?? "";
  }

  const todayValue = todayIso();
  if (end && end > todayValue) {
    end = todayValue;
  }

  return { start, end };
}
