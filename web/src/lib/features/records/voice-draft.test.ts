import { describe, expect, it } from "vitest";
import type { components } from "$lib/api/schema";
import { computeAutoShares } from "$lib/features/splits/allocation";
import { voiceDraftFields } from "./voice-draft";

type Draft = components["schemas"]["VoiceEntryResponse"];
const categories = [
  { id: "food", name: "餐飲", is_income: false },
  { id: "salary", name: "薪水", is_income: true },
];
const friends = [
  { id: "relation-a", user_id: "a", nickname: "小明", pending: false },
  { id: "relation-b", user_id: "b", nickname: "小華", pending: false },
];
const draft: Draft = {
  transcript: "昨天晚餐九百",
  name: "晚餐",
  total_amount: 900,
  date: "2026-10-21",
  category_id: "food",
  split_mode: "none",
  friend_ids: [],
  friend_amounts: [],
};

describe("voiceDraftFields", () => {
  it("fills ordinary entries and derives income/expense from the category", () => {
    expect(voiceDraftFields(draft, categories, friends, "cents", "TWD")).toMatchObject({
      name: "晚餐",
      amountInput: "900",
      isIncome: false,
      categoryId: "food",
      date: "2026-10-21",
      splitEnabled: false,
    });
    expect(
      voiceDraftFields({ ...draft, category_id: "salary" }, categories, friends, "cents", "TWD")
        .isIncome,
    ).toBe(true);
  });

  it("seeds equal shares including the payer using the existing allocation code", () => {
    const fields = voiceDraftFields(
      {
        ...draft,
        total_amount: 10,
        split_mode: "equal",
        friend_ids: ["a", "b"],
      },
      categories,
      friends,
      "cents",
      "TWD",
    );
    expect(fields.splitEnabled).toBe(true);
    expect(fields.splitPrefill).toEqual({ friendIds: ["a", "b"], amounts: {} });
    expect(
      computeAutoShares({
        selectedIds: fields.splitPrefill.friendIds,
        lockedAmounts: fields.splitPrefill.amounts,
        total: 10,
        touched: new Set(),
        mode: "cents",
        currency: "TWD",
      }),
    ).toEqual({ a: "3", b: "3" });
  });

  it("locks explicit friend amounts instead of recomputing them", () => {
    const fields = voiceDraftFields(
      {
        ...draft,
        split_mode: "explicit",
        friend_ids: ["a", "b"],
        friend_amounts: [
          { user_id: "a", amount: 250 },
          { user_id: "b", amount: 300 },
        ],
      },
      categories,
      friends,
      "cents",
      "TWD",
    );
    expect(
      computeAutoShares({
        selectedIds: fields.splitPrefill.friendIds,
        lockedAmounts: fields.splitPrefill.amounts,
        total: 900,
        touched: new Set(Object.keys(fields.splitPrefill.amounts)),
        mode: "cents",
        currency: "TWD",
      }),
    ).toEqual({ a: "250", b: "300" });
  });

  it("preserves decimal amounts for currencies that support them", () => {
    const fields = voiceDraftFields(
      { ...draft, total_amount: 900.5 },
      categories,
      friends,
      "cents",
      "USD",
    );
    expect(fields.amountInput).toBe("900.50");
    expect(
      computeAutoShares({
        selectedIds: ["a", "b"],
        total: 10,
        lockedAmounts: {},
        touched: new Set(),
        mode: "cents",
        currency: "USD",
      }),
    ).toEqual({ a: "3.33", b: "3.33" });
  });

  it("does not silently truncate spoken decimal amounts in whole mode or JPY", () => {
    expect(() =>
      voiceDraftFields({ ...draft, total_amount: 900.5 }, categories, friends, "whole", "TWD"),
    ).toThrow("decimal precision");
    expect(() =>
      voiceDraftFields({ ...draft, total_amount: 900.5 }, categories, friends, "cents", "JPY"),
    ).toThrow("decimal precision");
  });

  it("rejects stale category or friend data before applying any field", () => {
    expect(() =>
      voiceDraftFields({ ...draft, category_id: "gone" }, categories, friends, "cents", "TWD"),
    ).toThrow("Reload");
    expect(() =>
      voiceDraftFields({ ...draft, friend_ids: ["stranger"] }, categories, friends, "cents", "TWD"),
    ).toThrow("Reload");
  });
});
