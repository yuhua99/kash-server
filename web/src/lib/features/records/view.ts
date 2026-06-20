import type { components } from "$lib/api/schema";

type RecordItem = components["schemas"]["Record"];

export type SortMode = "date_desc" | "date_asc" | "amount_desc" | "amount_asc";

export type CategoryFilterMode = "all_expenses" | "all_incomes" | `category:${string}`;

export type SpendSummary = { currency: string; amount: number };

export type DateGroup = {
  date: string;
  records: RecordItem[];
  spendSummaries: SpendSummary[];
};

export function matchesRecordFilters(
  record: RecordItem,
  filters: { normalizedSearch: string; categoryFilter: CategoryFilterMode },
): boolean {
  const { normalizedSearch, categoryFilter } = filters;

  if (normalizedSearch && !record.name.toLowerCase().includes(normalizedSearch)) {
    return false;
  }

  if (categoryFilter === "all_expenses") {
    return record.amount < 0;
  }
  if (categoryFilter === "all_incomes") {
    return record.amount > 0;
  }
  return record.category_id === categoryFilter.slice("category:".length);
}

function convertedAbs(record: RecordItem, convertedById: Map<string, number>): number {
  const converted = convertedById.get(record.id);
  return Math.abs(converted ?? record.amount);
}

export function compareRecords(
  a: RecordItem,
  b: RecordItem,
  mode: SortMode,
  convertedById: Map<string, number>,
): number {
  switch (mode) {
    case "date_asc":
      return a.date < b.date ? -1 : a.date > b.date ? 1 : 0;
    case "date_desc":
      return a.date < b.date ? 1 : a.date > b.date ? -1 : 0;
    case "amount_asc": {
      const diff = convertedAbs(a, convertedById) - convertedAbs(b, convertedById);
      return diff !== 0 ? diff : a.amount - b.amount;
    }
    case "amount_desc": {
      const diff = convertedAbs(b, convertedById) - convertedAbs(a, convertedById);
      return diff !== 0 ? diff : b.amount - a.amount;
    }
  }
}

export function summarizeDailySpend(
  records: RecordItem[],
  convertedSpendById: Map<string, number>,
  mainCurrency: string,
): SpendSummary[] {
  const expenses = records.filter((record) => record.amount < 0);

  if (mainCurrency) {
    let total = 0;
    for (const record of expenses) {
      total +=
        record.currency === mainCurrency
          ? Math.abs(record.amount)
          : (convertedSpendById.get(record.id) ?? Math.abs(record.amount));
    }
    return total > 0 ? [{ currency: mainCurrency, amount: total }] : [];
  }

  const subtotals = new Map<string, number>();
  for (const record of expenses) {
    subtotals.set(record.currency, (subtotals.get(record.currency) ?? 0) + Math.abs(record.amount));
  }
  return [...subtotals.entries()]
    .map(([currency, amount]) => ({ currency, amount }))
    .sort((a, b) => a.currency.localeCompare(b.currency));
}

export function groupRecordsByDate(
  records: RecordItem[],
  mode: SortMode,
  convertedSpendById: Map<string, number>,
  mainCurrency: string,
): DateGroup[] {
  const groups = new Map<string, RecordItem[]>();
  for (const record of records) {
    const bucket = groups.get(record.date) ?? [];
    bucket.push(record);
    groups.set(record.date, bucket);
  }

  const descending = mode === "date_desc" || mode === "amount_desc";
  const dates = [...groups.keys()].sort((a, b) =>
    descending ? (a < b ? 1 : a > b ? -1 : 0) : a < b ? -1 : a > b ? 1 : 0,
  );

  return dates.map((date) => {
    const groupRecords = groups.get(date) ?? [];
    return {
      date,
      records: groupRecords,
      spendSummaries: summarizeDailySpend(groupRecords, convertedSpendById, mainCurrency),
    };
  });
}
