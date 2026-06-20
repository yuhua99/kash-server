import { describe, expect, it } from "vitest";
import type { components } from "$lib/api/schema";
import {
  compareRecords,
  groupRecordsByDate,
  matchesRecordFilters,
  summarizeDailySpend,
} from "./view";

type RecordItem = components["schemas"]["Record"];

function rec(partial: Partial<RecordItem> & { id: string }): RecordItem {
  return {
    id: partial.id,
    name: partial.name ?? "Item",
    amount: partial.amount ?? -10,
    currency: partial.currency ?? "USD",
    category_id: partial.category_id ?? "c1",
    date: partial.date ?? "2024-01-01",
  };
}

describe("matchesRecordFilters", () => {
  it("filters by case-insensitive name search", () => {
    const r = rec({ id: "1", name: "Coffee" });
    expect(
      matchesRecordFilters(r, { normalizedSearch: "coff", categoryFilter: "all_expenses" }),
    ).toBe(true);
    expect(
      matchesRecordFilters(r, { normalizedSearch: "tea", categoryFilter: "all_expenses" }),
    ).toBe(false);
  });

  it("filters expenses, incomes, and category", () => {
    const expense = rec({ id: "1", amount: -5, category_id: "food" });
    const income = rec({ id: "2", amount: 5, category_id: "salary" });
    expect(
      matchesRecordFilters(expense, { normalizedSearch: "", categoryFilter: "all_expenses" }),
    ).toBe(true);
    expect(
      matchesRecordFilters(income, { normalizedSearch: "", categoryFilter: "all_expenses" }),
    ).toBe(false);
    expect(
      matchesRecordFilters(income, { normalizedSearch: "", categoryFilter: "all_incomes" }),
    ).toBe(true);
    expect(
      matchesRecordFilters(expense, { normalizedSearch: "", categoryFilter: "category:food" }),
    ).toBe(true);
    expect(
      matchesRecordFilters(expense, { normalizedSearch: "", categoryFilter: "category:salary" }),
    ).toBe(false);
  });
});

describe("compareRecords", () => {
  const a = rec({ id: "a", amount: -10, date: "2024-01-01" });
  const b = rec({ id: "b", amount: -30, date: "2024-01-02" });

  it("sorts by date", () => {
    expect(compareRecords(a, b, "date_desc", new Map()) > 0).toBe(true);
    expect(compareRecords(a, b, "date_asc", new Map()) < 0).toBe(true);
  });

  it("sorts by converted absolute amount", () => {
    const converted = new Map([
      ["a", -100],
      ["b", -30],
    ]);
    expect(compareRecords(a, b, "amount_desc", converted) < 0).toBe(true);
    expect(compareRecords(a, b, "amount_asc", converted) > 0).toBe(true);
  });

  it("falls back to raw amount when no conversion", () => {
    expect(compareRecords(a, b, "amount_desc", new Map()) > 0).toBe(true);
  });
});

describe("summarizeDailySpend", () => {
  it("totals converted expenses in the main currency", () => {
    const records = [
      rec({ id: "1", amount: -10, currency: "USD" }),
      rec({ id: "2", amount: -20, currency: "EUR" }),
      rec({ id: "3", amount: 50, currency: "USD" }),
    ];
    const converted = new Map([["2", 22]]);
    expect(summarizeDailySpend(records, converted, "USD")).toEqual([
      { currency: "USD", amount: 32 },
    ]);
  });

  it("returns per-currency subtotals when no main currency", () => {
    const records = [
      rec({ id: "1", amount: -10, currency: "USD" }),
      rec({ id: "2", amount: -5, currency: "EUR" }),
      rec({ id: "3", amount: -4, currency: "USD" }),
    ];
    expect(summarizeDailySpend(records, new Map(), "")).toEqual([
      { currency: "EUR", amount: 5 },
      { currency: "USD", amount: 14 },
    ]);
  });
});

describe("groupRecordsByDate", () => {
  it("groups by date and sorts dates per mode", () => {
    const records = [
      rec({ id: "1", date: "2024-01-01", amount: -10, currency: "USD" }),
      rec({ id: "2", date: "2024-01-02", amount: -20, currency: "USD" }),
    ];
    const desc = groupRecordsByDate(records, "date_desc", new Map(), "USD");
    expect(desc.map((g) => g.date)).toEqual(["2024-01-02", "2024-01-01"]);
    const asc = groupRecordsByDate(records, "date_asc", new Map(), "USD");
    expect(asc.map((g) => g.date)).toEqual(["2024-01-01", "2024-01-02"]);
    expect(desc[0].spendSummaries).toEqual([{ currency: "USD", amount: 20 }]);
  });
});
