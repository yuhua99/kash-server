import { describe, expect, it } from "vitest";
import { buildCurrencySubtotals, buildRateLookup, convertAmountToMainCurrency } from "./fx";

describe("buildRateLookup", () => {
  it("keys rates by date and currency", () => {
    const rates = buildRateLookup([{ date: "2024-01-02", currency: "USD", rate: 30 }]);

    expect(rates.get("2024-01-02:USD")).toBe(30);
  });
});

describe("convertAmountToMainCurrency", () => {
  it("returns the original amount when currencies match", () => {
    expect(convertAmountToMainCurrency(10, "USD", "USD", "2024-01-02", new Map())).toBe(10);
  });

  it("converts using the target rate divided by the source rate", () => {
    const rates = buildRateLookup([
      { date: "2024-01-02", currency: "USD", rate: 2 },
      { date: "2024-01-02", currency: "TWD", rate: 6 },
    ]);

    expect(convertAmountToMainCurrency(10, "USD", "TWD", "2024-01-02", rates)).toBe(30);
  });

  it("throws when a rate is missing", () => {
    const rates = buildRateLookup([{ date: "2024-01-02", currency: "USD", rate: 2 }]);

    expect(() => convertAmountToMainCurrency(10, "USD", "TWD", "2024-01-02", rates)).toThrow(
      "Missing FX rate for 2024-01-02",
    );
  });
});

describe("buildCurrencySubtotals", () => {
  it("aggregates totals and sorts by currency", () => {
    expect(
      buildCurrencySubtotals([
        { currency: "USD", amount: 10 },
        { currency: "TWD", amount: 20 },
        { currency: "USD", amount: -3 },
        { currency: "EUR", amount: 5 },
      ]),
    ).toEqual([
      { currency: "EUR", total: 5 },
      { currency: "TWD", total: 20 },
      { currency: "USD", total: 7 },
    ]);
  });
});
