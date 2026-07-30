import { describe, expect, it } from "vitest";
import {
  amountInputStep,
  formatAmountInput,
  formatMoney,
  normalizeAmountInputValue,
  roundToCents,
} from "./amount-display";

describe("roundToCents", () => {
  it("rounds to cents", () => {
    expect(roundToCents(1.236)).toBe(1.24);
    expect(roundToCents(1.231)).toBe(1.23);
  });
});

describe("normalizeAmountInputValue", () => {
  it("truncates in whole mode", () => {
    expect(normalizeAmountInputValue(12.9, "whole")).toBe(12);
    expect(normalizeAmountInputValue(-12.9, "whole")).toBe(-12);
  });

  it("rounds in cents mode", () => {
    expect(normalizeAmountInputValue(12.344, "cents")).toBe(12.34);
  });

  it("rounds zero-decimal currencies to whole amounts", () => {
    expect(normalizeAmountInputValue(12.6, "cents", "TWD")).toBe(13);
  });
});

describe("formatAmountInput", () => {
  it("formats whole mode", () => {
    expect(formatAmountInput(12.9, "whole")).toBe("12");
  });

  it("formats cents mode with a two-decimal currency", () => {
    expect(formatAmountInput(12.5, "cents", "USD")).toBe("12.50");
  });

  it("formats cents mode with a zero-decimal currency", () => {
    expect(formatAmountInput(12.6, "cents", "TWD")).toBe("13");
    expect(amountInputStep("cents", "TWD")).toBe("1");
  });

  it("formats cents mode without a currency", () => {
    expect(formatAmountInput(1.2, "cents")).toBe("1.20");
  });
});

describe("formatMoney", () => {
  it("formats whole mode with grouping", () => {
    expect(formatMoney(1234.9, "whole", "USD")).toBe("1,234");
  });

  it("formats cents mode with currency fraction digits", () => {
    expect(formatMoney(1234.5, "cents", "USD")).toBe("1,234.50");
    expect(formatMoney(1234, "cents", "TWD")).toBe("1,234");
  });

  it("adds a plus sign for positive signed amounts", () => {
    expect(formatMoney(12.5, "cents", "USD", { signed: true })).toBe("+12.50");
    expect(formatMoney(-12.5, "cents", "USD", { signed: true })).toBe("-12.50");
    expect(formatMoney(0, "cents", "USD", { signed: true })).toBe("0.00");
  });

  it("omits signs for sub-unit values that round to zero", () => {
    expect(formatMoney(0.001, "cents", "USD", { signed: true })).toBe("0.00");
    expect(formatMoney(-0.001, "cents", "USD", { signed: true })).toBe("0.00");
    expect(formatMoney(0.9, "whole", "USD", { signed: true })).toBe("0");
    expect(formatMoney(-0.9, "whole", "USD", { signed: true })).toBe("0");
  });
});
