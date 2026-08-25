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
    throw new Error(`Exchange rate unavailable for ${date}`);
  }

  return amount * (toRate / fromRate);
}
