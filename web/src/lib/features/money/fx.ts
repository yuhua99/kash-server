export type ExchangeRateRow = { date: string; currency: string; rate: number };

export function buildRateLookup(rows: ExchangeRateRow[]): Map<string, number> {
  return new Map(rows.map((row) => [`${row.date}:${row.currency}`, row.rate]));
}

export function convertAmountToMainCurrency(
  amount: number,
  from: string,
  to: string,
  date: string,
  rates: Map<string, number>,
): number {
  if (from === to) {
    return amount;
  }

  const fromRate = rates.get(`${date}:${from}`);
  const toRate = rates.get(`${date}:${to}`);

  if (fromRate === undefined || toRate === undefined) {
    throw new Error(`Missing FX rate for ${date}`);
  }

  return amount * (toRate / fromRate);
}

export function buildCurrencySubtotals(
  items: { currency: string; amount: number }[],
): { currency: string; total: number }[] {
  const totals = new Map<string, number>();

  for (const item of items) {
    totals.set(item.currency, (totals.get(item.currency) ?? 0) + item.amount);
  }

  return Array.from(totals, ([currency, total]) => ({ currency, total })).sort((a, b) =>
    a.currency.localeCompare(b.currency),
  );
}
