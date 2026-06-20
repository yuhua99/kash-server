import { getFxRates } from "$lib/features/money/rates";
import { buildRateLookup, convertAmountToMainCurrency } from "$lib/features/money/fx";

interface ConvertableRecord {
  id: string;
  amount: number;
  currency: string;
  date: string;
}

export interface ConversionResult {
  convertedById: Map<string, number>;
  convertedSpendById: Map<string, number>;
  displayCurrency: string;
}

export async function convertRecords(
  records: ConvertableRecord[],
  mainCurrency: string,
  dateRange: { from: string; to: string },
): Promise<ConversionResult> {
  if (!mainCurrency) {
    return { convertedById: new Map(), convertedSpendById: new Map(), displayCurrency: "" };
  }

  const quotes = new Set(records.map((r) => r.currency));
  quotes.add(mainCurrency);

  const rates = await getFxRates({ from: dateRange.from, to: dateRange.to, quotes: [...quotes] });
  const lookup = buildRateLookup(rates);
  const convertedById = new Map<string, number>();
  const convertedSpendById = new Map<string, number>();

  for (const record of records) {
    const value = convertAmountToMainCurrency(
      record.amount,
      record.currency,
      mainCurrency,
      record.date,
      lookup,
    );
    convertedById.set(record.id, value);
    if (record.amount < 0) {
      convertedSpendById.set(record.id, Math.abs(value));
    }
  }

  return { convertedById, convertedSpendById, displayCurrency: mainCurrency };
}
