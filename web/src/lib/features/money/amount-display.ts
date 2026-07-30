import { writable } from "svelte/store";
import { getCurrencyConfig } from "./currency";

export type AmountDisplayMode = "cents" | "whole";

const STORAGE_KEY = "kash_amount_display_mode";
const DEFAULT_MODE: AmountDisplayMode = "cents";

function readStored(): AmountDisplayMode {
  if (typeof localStorage !== "undefined") {
    const stored = localStorage.getItem(STORAGE_KEY);

    if (stored === "whole" || stored === "cents") {
      return stored;
    }
  }

  return DEFAULT_MODE;
}

export const amountDisplayMode = writable<AmountDisplayMode>(readStored());

export function setAmountDisplayMode(mode: AmountDisplayMode): void {
  amountDisplayMode.set(mode);

  if (typeof localStorage !== "undefined") {
    localStorage.setItem(STORAGE_KEY, mode);
  }
}

export function roundToCents(n: number): number {
  return Math.round(n * 100) / 100;
}

function fractionDigitsFor(mode: AmountDisplayMode, currency?: string): number {
  return mode === "whole" ? 0 : currency ? getCurrencyConfig(currency).fractionDigits : 2;
}

export function amountInputStep(mode: AmountDisplayMode, currency?: string): string {
  return String(10 ** -fractionDigitsFor(mode, currency));
}

export function normalizeAmountInputValue(
  value: number,
  mode: AmountDisplayMode,
  currency?: string,
): number {
  if (mode === "whole") {
    return Math.trunc(value);
  }

  return fractionDigitsFor(mode, currency) === 0 ? Math.round(value) : roundToCents(value);
}

export function formatAmountInput(
  value: number,
  mode: AmountDisplayMode,
  currency?: string,
): string {
  if (mode === "whole") {
    return String(Math.trunc(value));
  }

  return value.toFixed(fractionDigitsFor(mode, currency));
}

export function formatMoney(
  value: number,
  mode: AmountDisplayMode,
  currency?: string,
  options: { signed?: boolean } = {},
): string {
  const fractionDigits = fractionDigitsFor(mode, currency);
  const amount = mode === "whole" ? Math.trunc(value) || 0 : value;
  const formatter = new Intl.NumberFormat("en-US", {
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  });
  const parts = formatter.formatToParts(amount);
  const isZero = parts
    .filter((part) => part.type === "integer" || part.type === "fraction")
    .every((part) => /^0+$/.test(part.value));
  const formatted = parts
    .filter((part) => !isZero || part.type !== "minusSign")
    .map((part) => part.value)
    .join("");

  return `${options.signed && value > 0 && !isZero ? "+" : ""}${formatted}`;
}
