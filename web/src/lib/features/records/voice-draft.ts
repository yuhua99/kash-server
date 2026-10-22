import type { components } from "$lib/api/schema";
import {
  formatAmountInput,
  normalizeAmountInputValue,
  type AmountDisplayMode,
} from "$lib/features/money/amount-display";
import type { SplitPrefill } from "$lib/features/splits/allocation";

export function voiceDraftFields(
  draft: components["schemas"]["VoiceEntryResponse"],
  categories: components["schemas"]["Category"][],
  friends: components["schemas"]["FriendshipRelation"][],
  mode: AmountDisplayMode,
  currency: string,
) {
  const category = categories.find((item) => item.id === draft.category_id);
  if (
    !category ||
    draft.friend_ids.some((id) => !friends.some((friend) => friend.user_id === id))
  ) {
    throw new Error(
      "Categories or friends have changed. Reload the page before using voice entry.",
    );
  }

  const amounts = [draft.total_amount, ...draft.friend_amounts.map((friend) => friend.amount)];
  if (amounts.some((amount) => normalizeAmountInputValue(amount, mode, currency) !== amount)) {
    throw new Error(
      "Recorded amounts need decimal precision. Change the amount display mode or enter them manually.",
    );
  }

  const splitPrefill: SplitPrefill = {
    friendIds: draft.friend_ids,
    amounts: Object.fromEntries(
      draft.friend_amounts.map((friend) => [friend.user_id, friend.amount]),
    ),
  };
  return {
    name: draft.name,
    amountInput: formatAmountInput(draft.total_amount, mode, currency),
    isIncome: category.is_income,
    categoryId: category.id,
    date: draft.date,
    splitEnabled: draft.split_mode !== "none",
    splitPrefill,
  };
}
