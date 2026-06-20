export type SupportedCurrencyCode = "TWD" | "USD" | "JPY" | "EUR" | "CNY";

export type CurrencyConfig = {
  code: SupportedCurrencyCode;
  fractionDigits: number;
};

const CURRENCY_CONFIG: Record<SupportedCurrencyCode, CurrencyConfig> = {
  TWD: { code: "TWD", fractionDigits: 0 },
  USD: { code: "USD", fractionDigits: 2 },
  JPY: { code: "JPY", fractionDigits: 0 },
  EUR: { code: "EUR", fractionDigits: 2 },
  CNY: { code: "CNY", fractionDigits: 2 },
};

export const SUPPORTED_CURRENCIES: SupportedCurrencyCode[] = ["TWD", "USD", "JPY", "EUR", "CNY"];

export const DEFAULT_CURRENCY_CODE: SupportedCurrencyCode = "TWD";

export function isSupportedCurrencyCode(value: string): value is SupportedCurrencyCode {
  return SUPPORTED_CURRENCIES.includes(value as SupportedCurrencyCode);
}

export function getCurrencyConfig(code: string): CurrencyConfig {
  return isSupportedCurrencyCode(code)
    ? CURRENCY_CONFIG[code]
    : CURRENCY_CONFIG[DEFAULT_CURRENCY_CODE];
}

export function formatMoney(amount: number, code: string): string {
  const fractionDigits = getCurrencyConfig(code).fractionDigits;

  return new Intl.NumberFormat("en-US", {
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  }).format(amount);
}

export function formatSignedMoney(amount: number, code: string): string {
  return `${amount > 0 ? "+" : ""}${formatMoney(amount, code)}`;
}
