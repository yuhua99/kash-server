import { describe, expect, it } from "vitest";
import {
  DEFAULT_CURRENCY_CODE,
  formatMoney,
  formatSignedMoney,
  getCurrencyConfig,
  isSupportedCurrencyCode,
} from "./currency";

describe("currency", () => {
  it("identifies supported currency codes", () => {
    expect(isSupportedCurrencyCode("TWD")).toBe(true);
    expect(isSupportedCurrencyCode("USD")).toBe(true);
    expect(isSupportedCurrencyCode("JPY")).toBe(true);
    expect(isSupportedCurrencyCode("EUR")).toBe(true);
    expect(isSupportedCurrencyCode("CNY")).toBe(true);
  });

  it("rejects unsupported currency codes", () => {
    expect(isSupportedCurrencyCode("GBP")).toBe(false);
    expect(isSupportedCurrencyCode("")).toBe(false);
  });

  it("gets known currency config", () => {
    expect(getCurrencyConfig("USD")).toEqual({ code: "USD", fractionDigits: 2 });
    expect(getCurrencyConfig("JPY")).toEqual({ code: "JPY", fractionDigits: 0 });
  });

  it("falls back to default currency config for unknown codes", () => {
    expect(getCurrencyConfig("GBP")).toEqual({ code: DEFAULT_CURRENCY_CODE, fractionDigits: 0 });
  });

  it("formats zero-decimal currencies", () => {
    expect(formatMoney(1234, "JPY")).toBe("1,234");
    expect(formatMoney(1234, "TWD")).toBe("1,234");
  });

  it("formats two-decimal currencies", () => {
    expect(formatMoney(1234.5, "USD")).toBe("1,234.50");
    expect(formatMoney(1234.5, "EUR")).toBe("1,234.50");
    expect(formatMoney(1234.5, "CNY")).toBe("1,234.50");
  });

  it("formats signed money", () => {
    expect(formatSignedMoney(1234.5, "USD")).toBe("+1,234.50");
    expect(formatSignedMoney(-1234.5, "USD")).toBe("-1,234.50");
    expect(formatSignedMoney(0, "USD")).toBe("0.00");
  });
});
