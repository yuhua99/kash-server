import { describe, expect, it } from "vitest";
import type { components } from "$lib/api/schema";
import { buildBreakdown, calculateTotals } from "./query";

type RecordItem = components["schemas"]["Record"];
type Category = components["schemas"]["Category"];

function rec(id: string, amount: number, categoryId: string | null): RecordItem {
  return { id, name: id, amount, currency: "USD", category_id: categoryId, date: "2024-01-01" };
}

const categories: Category[] = [
  { id: "food", name: "Food", is_income: false },
  { id: "salary", name: "Salary", is_income: true },
];

describe("calculateTotals", () => {
  it("computes net, income, and expense totals", () => {
    const records = [rec("1", 100, "salary"), rec("2", -30, "food"), rec("3", -20, "food")];
    expect(calculateTotals(records)).toEqual({ netTotal: 50, incomeTotal: 100, expenseTotal: 50 });
  });

  it("returns zeros for no records", () => {
    expect(calculateTotals([])).toEqual({ netTotal: 0, incomeTotal: 0, expenseTotal: 0 });
  });
});

describe("buildBreakdown", () => {
  it("aggregates by category with shares summing to one", () => {
    const records = [rec("1", -30, "food"), rec("2", -10, "food"), rec("3", 100, "salary")];
    const breakdown = buildBreakdown(records, categories);
    expect(breakdown.map((item) => item.categoryId)).toEqual(["salary", "food"]);
    const salary = breakdown.find((item) => item.categoryId === "salary");
    const food = breakdown.find((item) => item.categoryId === "food");
    expect(salary).toMatchObject({
      name: "Salary",
      isIncome: true,
      total: 100,
      absoluteTotal: 100,
    });
    expect(food).toMatchObject({ name: "Food", isIncome: false, total: -40, absoluteTotal: 40 });
    expect(salary!.share + food!.share).toBeCloseTo(1);
  });

  it("groups uncategorized records", () => {
    const breakdown = buildBreakdown([rec("1", -5, null)], categories);
    expect(breakdown[0]).toMatchObject({ categoryId: "", name: "Uncategorized", isIncome: false });
  });
});
