import { describe, expect, it } from "vitest";
import {
  validateAmount,
  validateCategoryName,
  validateDate,
  validateFriendSearchQuery,
  validateNickname,
  validatePassword,
  validateRecordName,
  validateSearchTerm,
  validateSplitParticipantAmount,
  validateSplitTotals,
  validateUsername,
} from "./validation";

describe("validateUsername", () => {
  it("accepts valid usernames at length boundaries", () => {
    expect(validateUsername("abcd")).toBeNull();
    expect(validateUsername("a".repeat(50))).toBeNull();
  });

  it("rejects usernames shorter than 4 characters", () => {
    expect(validateUsername("abc")).toBe("Username must be 4-50 characters.");
  });

  it("rejects usernames longer than 50 characters", () => {
    expect(validateUsername("a".repeat(51))).toBe("Username must be 4-50 characters.");
  });

  it("rejects invalid username characters", () => {
    expect(validateUsername("abcd!")).toBe("Username allows letters, numbers, _ and - only.");
  });
});

describe("validatePassword", () => {
  it("accepts passwords with at least 6 characters", () => {
    expect(validatePassword("123456")).toBeNull();
  });

  it("rejects passwords shorter than 6 characters", () => {
    expect(validatePassword("12345")).toBe("Password must be at least 6 characters.");
  });
});

describe("validateCategoryName", () => {
  it("accepts category names at length boundaries", () => {
    expect(validateCategoryName("a")).toBeNull();
    expect(validateCategoryName("a".repeat(100))).toBeNull();
  });

  it("rejects empty category names", () => {
    expect(validateCategoryName("   ")).toBe("Category name must be 1-100 characters.");
  });

  it("rejects category names longer than 100 characters", () => {
    expect(validateCategoryName("a".repeat(101))).toBe("Category name must be 1-100 characters.");
  });

  it("counts raw length including trailing whitespace", () => {
    expect(validateCategoryName("a".repeat(100) + " ")).toBe(
      "Category name must be 1-100 characters.",
    );
  });
});

describe("validateRecordName", () => {
  it("accepts record names at length boundaries", () => {
    expect(validateRecordName("a")).toBeNull();
    expect(validateRecordName("a".repeat(255))).toBeNull();
  });

  it("rejects empty record names", () => {
    expect(validateRecordName("   ")).toBe("Record name must be 1-255 characters.");
  });

  it("rejects record names longer than 255 characters", () => {
    expect(validateRecordName("a".repeat(256))).toBe("Record name must be 1-255 characters.");
  });
});

describe("validateSearchTerm", () => {
  it("accepts empty and valid search terms", () => {
    expect(validateSearchTerm("   ")).toBeNull();
    expect(validateSearchTerm("a".repeat(100))).toBeNull();
  });

  it("rejects search terms longer than 100 characters", () => {
    expect(validateSearchTerm("a".repeat(101))).toBe("Search term must be 1-100 characters.");
  });
});

describe("validateDate", () => {
  it("accepts YYYY-MM-DD dates", () => {
    expect(validateDate("2026-06-21")).toBeNull();
  });

  it("rejects dates in other formats", () => {
    expect(validateDate("2026/06/21")).toBe("Date must use YYYY-MM-DD.");
  });
});

describe("validateAmount", () => {
  it("accepts non-zero finite amounts", () => {
    expect(validateAmount(12.34)).toBeNull();
  });

  it("rejects NaN", () => {
    expect(validateAmount(Number.NaN)).toBe("Amount must be a number.");
  });

  it("rejects Infinity", () => {
    expect(validateAmount(Number.POSITIVE_INFINITY)).toBe("Amount must be a number.");
  });

  it("rejects zero", () => {
    expect(validateAmount(0)).toBe("Amount cannot be 0.");
  });
});

describe("validateNickname", () => {
  it("accepts empty and 50-character nicknames", () => {
    expect(validateNickname("   ")).toBeNull();
    expect(validateNickname("a".repeat(50))).toBeNull();
  });

  it("rejects nicknames longer than 50 characters", () => {
    expect(validateNickname("a".repeat(51))).toBe("Nickname must be 50 characters or fewer.");
  });

  it("counts raw length including trailing whitespace", () => {
    expect(validateNickname("a".repeat(50) + " ")).toBe("Nickname must be 50 characters or fewer.");
  });
});

describe("validateFriendSearchQuery", () => {
  it("accepts search queries at length boundaries", () => {
    expect(validateFriendSearchQuery("a")).toBeNull();
    expect(validateFriendSearchQuery("a".repeat(50))).toBeNull();
  });

  it("rejects empty search queries", () => {
    expect(validateFriendSearchQuery("   ")).toBe("Search query is required.");
  });

  it("rejects search queries longer than 50 characters", () => {
    expect(validateFriendSearchQuery("a".repeat(51))).toBe(
      "Search query must be 50 characters or fewer.",
    );
  });
});

describe("validateSplitParticipantAmount", () => {
  it("accepts positive finite amounts", () => {
    expect(validateSplitParticipantAmount(0.01)).toBeNull();
  });

  it("rejects NaN", () => {
    expect(validateSplitParticipantAmount(Number.NaN)).toBe("Amount must be greater than 0.");
  });

  it("rejects Infinity", () => {
    expect(validateSplitParticipantAmount(Number.POSITIVE_INFINITY)).toBe(
      "Amount must be greater than 0.",
    );
  });

  it("rejects zero", () => {
    expect(validateSplitParticipantAmount(0)).toBe("Amount must be greater than 0.");
  });

  it("rejects negative amounts", () => {
    expect(validateSplitParticipantAmount(-1)).toBe("Amount must be greater than 0.");
  });
});

describe("validateSplitTotals", () => {
  it("accepts totals equal in cents", () => {
    expect(validateSplitTotals(10, 10)).toBeNull();
  });

  it("accepts totals under by one cent", () => {
    expect(validateSplitTotals(9.99, 10)).toBeNull();
  });

  it("rejects totals exceeding by one cent", () => {
    expect(validateSplitTotals(10.01, 10)).toBe("Participant shares cannot exceed total amount.");
  });
});
