import { writable } from "svelte/store";
import {
  DEFAULT_CURRENCY_CODE,
  isSupportedCurrencyCode,
  type SupportedCurrencyCode,
} from "./currency";

const STORAGE_KEY = "kash_current_currency";

function readStored(): SupportedCurrencyCode {
  if (typeof localStorage !== "undefined") {
    const stored = localStorage.getItem(STORAGE_KEY);

    if (stored !== null && isSupportedCurrencyCode(stored)) {
      return stored;
    }
  }

  return DEFAULT_CURRENCY_CODE;
}

export const currentCurrency = writable<SupportedCurrencyCode>(readStored());

export function setCurrentCurrency(code: SupportedCurrencyCode): void {
  currentCurrency.set(code);

  if (typeof localStorage !== "undefined") {
    localStorage.setItem(STORAGE_KEY, code);
  }
}

export function initializeCurrentCurrency(defaultFromSettings: string): void {
  if (typeof localStorage !== "undefined") {
    const stored = localStorage.getItem(STORAGE_KEY);

    if (stored !== null && isSupportedCurrencyCode(stored)) {
      setCurrentCurrency(stored);
      return;
    }
  }

  if (isSupportedCurrencyCode(defaultFromSettings)) {
    setCurrentCurrency(defaultFromSettings);
    return;
  }

  setCurrentCurrency(DEFAULT_CURRENCY_CODE);
}
