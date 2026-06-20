import { describe, expect, it } from "vitest";
import { dateValueToIso, isoToDateValue, periodFromPreset, todayIso } from "./date";

describe("todayIso", () => {
  it("returns a local ISO date", () => {
    expect(todayIso()).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});

describe("isoToDateValue", () => {
  it("parses a valid ISO date", () => {
    expect(isoToDateValue("2024-02-29")?.toString()).toBe("2024-02-29");
  });

  it("returns undefined for empty input", () => {
    expect(isoToDateValue("")).toBeUndefined();
  });

  it("returns undefined for invalid input", () => {
    expect(isoToDateValue("not-a-date")).toBeUndefined();
  });
});

describe("dateValueToIso", () => {
  it("serializes a parsed value", () => {
    expect(dateValueToIso(isoToDateValue("2024-01-15"))).toBe("2024-01-15");
  });

  it("returns an empty string for nullish values", () => {
    expect(dateValueToIso(null)).toBe("");
    expect(dateValueToIso(undefined)).toBe("");
  });
});

describe("periodFromPreset", () => {
  it("returns a leap-year month range", () => {
    expect(periodFromPreset("month", { year: 2024, month: 2 })).toEqual({
      start: "2024-02-01",
      end: "2024-02-29",
    });
  });

  it("returns a past year range", () => {
    expect(periodFromPreset("year", { year: 2020 })).toEqual({
      start: "2020-01-01",
      end: "2020-12-31",
    });
  });

  it("passes through custom ranges", () => {
    expect(periodFromPreset("custom", { start: "2023-03-01", end: "2023-03-31" })).toEqual({
      start: "2023-03-01",
      end: "2023-03-31",
    });
  });

  it("caps future ends at today", () => {
    expect(periodFromPreset("custom", { start: "2999-01-01", end: "2999-12-31" })).toEqual({
      start: "2999-01-01",
      end: todayIso(),
    });
  });

  it("leaves an empty custom end empty", () => {
    expect(periodFromPreset("custom", { start: "2024-01-01" })).toEqual({
      start: "2024-01-01",
      end: "",
    });
  });

  it("returns ISO-shaped default month and year ranges", () => {
    expect(periodFromPreset("month")).toEqual({
      start: expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/),
      end: expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/),
    });
    expect(periodFromPreset("year")).toEqual({
      start: expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/),
      end: expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/),
    });
  });
});
