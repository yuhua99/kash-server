import {
  formatAmount,
  roundToCents,
  type AmountDisplayMode,
} from "$lib/features/money/amount-display";

function unitFor(mode: AmountDisplayMode): number {
  return mode === "whole" ? 1 : 0.01;
}

export function computeAutoShares(args: {
  selectedIds: string[];
  total: number;
  lockedAmounts: Record<string, number>;
  touched: Set<string>;
  mode: AmountDisplayMode;
}): Record<string, string> {
  const { selectedIds, total, lockedAmounts, touched, mode } = args;
  const unit = unitFor(mode);

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
      ? formatAmount(lockedAmounts[id] ?? 0, mode)
      : formatAmount(perShare, mode);
  }
  return result;
}

export function assignAllToFriends(args: {
  selectedIds: string[];
  total: number;
  mode: AmountDisplayMode;
}): Record<string, string> {
  const { selectedIds, total, mode } = args;
  const n = selectedIds.length;
  if (n === 0) {
    return {};
  }

  const unit = unitFor(mode);
  const totalUnits = Math.round(total / unit);
  const base = Math.floor(totalUnits / n);
  const remainder = totalUnits - base * n;

  const result: Record<string, string> = {};
  selectedIds.forEach((id, i) => {
    const units = base + (i < remainder ? 1 : 0);
    result[id] = formatAmount(units * unit, mode);
  });
  return result;
}

export function buildParticipantSplits(
  ids: string[],
  amountInputs: Record<string, string>,
): { user_id: string; amount: number }[] {
  return ids.map((id) => ({ user_id: id, amount: roundToCents(Number(amountInputs[id] ?? 0)) }));
}
