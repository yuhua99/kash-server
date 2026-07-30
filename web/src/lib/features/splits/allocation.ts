import {
  amountInputStep,
  formatAmountInput,
  normalizeAmountInputValue,
  roundToCents,
  type AmountDisplayMode,
} from "$lib/features/money/amount-display";

function unitFor(mode: AmountDisplayMode, currency?: string): number {
  return Number(amountInputStep(mode, currency));
}

export function computeAutoShares(args: {
  selectedIds: string[];
  total: number;
  lockedAmounts: Record<string, number>;
  touched: Set<string>;
  mode: AmountDisplayMode;
  currency?: string;
}): Record<string, string> {
  const { selectedIds, total, lockedAmounts, touched, mode, currency } = args;
  const unit = unitFor(mode, currency);

  const unlocked = selectedIds.filter((id) => !touched.has(id));
  const lockedSelected = selectedIds.filter((id) => touched.has(id));
  const lockedSum = lockedSelected.reduce((sum, id) => sum + (lockedAmounts[id] ?? 0), 0);

  const remaining = Math.max(0, total - lockedSum);
  const pool = unlocked.length + 1;
  const totalUnits = Math.round(remaining / unit);
  const perUnits = pool > 0 ? Math.floor(totalUnits / pool) : 0;
  const perShare = roundToCents(perUnits * unit);

  const result: Record<string, string> = {};
  for (const id of selectedIds) {
    result[id] = touched.has(id)
      ? formatAmountInput(lockedAmounts[id] ?? 0, mode, currency)
      : formatAmountInput(perShare, mode, currency);
  }
  return result;
}

export function assignAllToFriends(args: {
  selectedIds: string[];
  total: number;
  mode: AmountDisplayMode;
  currency?: string;
}): Record<string, string> {
  const { selectedIds, total, mode, currency } = args;
  const n = selectedIds.length;
  if (n === 0) {
    return {};
  }

  const unit = unitFor(mode, currency);
  const totalUnits = Math.round(total / unit);
  const base = Math.floor(totalUnits / n);
  const remainder = totalUnits - base * n;

  const result: Record<string, string> = {};
  selectedIds.forEach((id, i) => {
    const units = base + (i < remainder ? 1 : 0);
    result[id] = formatAmountInput(units * unit, mode, currency);
  });
  return result;
}

export function buildParticipantSplits(
  ids: string[],
  amountInputs: Record<string, string>,
  mode: AmountDisplayMode = "cents",
  currency?: string,
): { user_id: string; amount: number }[] {
  return ids.map((id) => ({
    user_id: id,
    amount: normalizeAmountInputValue(Number(amountInputs[id] ?? 0), mode, currency),
  }));
}
