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

export function normalizeAmountInputValue(value: number, mode: AmountDisplayMode): number {
  return mode === "whole" ? Math.trunc(value) : roundToCents(value);
}

export function formatAmount(value: number, mode: AmountDisplayMode, currency?: string): string {
  if (mode === "whole") {
    return String(Math.trunc(value));
  }

  const digits = currency ? getCurrencyConfig(currency).fractionDigits : 2;
  return value.toFixed(digits);
}

export function formatSignedAmount(
  value: number,
  mode: AmountDisplayMode,
  currency?: string,
): string {
  return `${value > 0 ? "+" : ""}${formatAmount(value, mode, currency)}`;
}
