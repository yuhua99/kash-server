import { describe, expect, it } from "vitest";
import {
  formatAmount,
  formatSignedAmount,
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
});

describe("formatAmount", () => {
  it("formats whole mode", () => {
    expect(formatAmount(12.9, "whole")).toBe("12");
  });

  it("formats cents mode with a two-decimal currency", () => {
    expect(formatAmount(12.5, "cents", "USD")).toBe("12.50");
  });

  it("formats cents mode with a zero-decimal currency", () => {
    expect(formatAmount(12, "cents", "TWD")).toBe("12");
  });

  it("formats cents mode without a currency", () => {
    expect(formatAmount(1.2, "cents")).toBe("1.20");
  });
});

describe("formatSignedAmount", () => {
  it("adds a plus sign for positive amounts", () => {
    expect(formatSignedAmount(12.5, "cents", "USD")).toBe("+12.50");
  });

  it("keeps the minus sign for negative amounts", () => {
    expect(formatSignedAmount(-12.5, "cents", "USD")).toBe("-12.50");
  });

  it("does not add a plus sign for zero", () => {
    expect(formatSignedAmount(0, "cents", "USD")).toBe("0.00");
  });
});
