import { describe, expect, it } from "vitest";
import { assignAllToFriends, buildParticipantSplits, computeAutoShares } from "./allocation";

describe("computeAutoShares", () => {
  it("splits equally including the payer", () => {
    expect(
      computeAutoShares({
        selectedIds: ["a", "b"],
        total: 30,
        lockedAmounts: {},
        touched: new Set(),
        mode: "whole",
      }),
    ).toEqual({ a: "10", b: "10" });
  });

  it("keeps locked amounts and splits the remainder among unlocked + payer", () => {
    expect(
      computeAutoShares({
        selectedIds: ["a", "b"],
        total: 30,
        lockedAmounts: { a: 20 },
        touched: new Set(["a"]),
        mode: "whole",
      }),
    ).toEqual({ a: "20", b: "5" });
  });

  it("floors to cents in cents mode", () => {
    expect(
      computeAutoShares({
        selectedIds: ["a", "b"],
        total: 10,
        lockedAmounts: {},
        touched: new Set(),
        mode: "cents",
      }),
    ).toEqual({ a: "3.33", b: "3.33" });
  });
});

describe("assignAllToFriends", () => {
  it("distributes cents remainder to the first friends", () => {
    expect(assignAllToFriends({ selectedIds: ["a", "b", "c"], total: 10, mode: "cents" })).toEqual({
      a: "3.34",
      b: "3.33",
      c: "3.33",
    });
  });

  it("distributes whole remainder to the first friends", () => {
    expect(assignAllToFriends({ selectedIds: ["a", "b", "c"], total: 10, mode: "whole" })).toEqual({
      a: "4",
      b: "3",
      c: "3",
    });
  });

  it("returns empty for no selection", () => {
    expect(assignAllToFriends({ selectedIds: [], total: 10, mode: "whole" })).toEqual({});
  });
});

describe("buildParticipantSplits", () => {
  it("maps ids and parses input strings to numbers", () => {
    expect(buildParticipantSplits(["a", "b"], { a: "3.34", b: "3.33" })).toEqual([
      { user_id: "a", amount: 3.34 },
      { user_id: "b", amount: 3.33 },
    ]);
  });
});
