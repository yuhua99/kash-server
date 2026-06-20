import type { components } from "$lib/api/schema";

type RecordItem = components["schemas"]["Record"];
type Category = components["schemas"]["Category"];

export type Totals = {
  netTotal: number;
  incomeTotal: number;
  expenseTotal: number;
};

export type BreakdownItem = {
  categoryId: string;
  name: string;
  isIncome: boolean;
  total: number;
  absoluteTotal: number;
  share: number;
};

export function calculateTotals(records: RecordItem[]): Totals {
  let incomeTotal = 0;
  let expenseTotal = 0;

  for (const record of records) {
    if (record.amount > 0) {
      incomeTotal += record.amount;
    } else if (record.amount < 0) {
      expenseTotal += Math.abs(record.amount);
    }
  }

  return { netTotal: incomeTotal - expenseTotal, incomeTotal, expenseTotal };
}

export function buildBreakdown(records: RecordItem[], categories: Category[]): BreakdownItem[] {
  const categoryById = new Map(categories.map((category) => [category.id, category]));
  const groups = new Map<string, { total: number; absoluteTotal: number }>();

  for (const record of records) {
    const key = record.category_id ?? "";
    const group = groups.get(key) ?? { total: 0, absoluteTotal: 0 };
    group.total += record.amount;
    group.absoluteTotal += Math.abs(record.amount);
    groups.set(key, group);
  }

  const totalAbsolute = [...groups.values()].reduce((sum, group) => sum + group.absoluteTotal, 0);

  return [...groups.entries()]
    .map(([categoryId, group]) => {
      const category = categoryById.get(categoryId);
      return {
        categoryId,
        name: category?.name ?? "Uncategorized",
        isIncome: category?.is_income ?? group.total > 0,
        total: group.total,
        absoluteTotal: group.absoluteTotal,
        share: totalAbsolute > 0 ? group.absoluteTotal / totalAbsolute : 0,
      };
    })
    .sort((a, b) => b.absoluteTotal - a.absoluteTotal);
}
