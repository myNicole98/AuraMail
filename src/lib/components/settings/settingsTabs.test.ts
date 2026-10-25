import { describe, expect, it } from "vitest";
import { nextTabIndex, SETTINGS_TABS } from "./settingsTabs";

describe("SETTINGS_TABS", () => {
  it("places accessibility directly after appearance", () => {
    expect(SETTINGS_TABS.map((tab) => tab.id)).toEqual([
      "general",
      "account",
      "appearance",
      "accessibility",
      "signatures",
      "ai"
    ]);
  });
});

describe("nextTabIndex", () => {
  it("moves right and wraps", () => {
    expect(nextTabIndex(0, "ArrowRight", 5)).toBe(1);
    expect(nextTabIndex(4, "ArrowRight", 5)).toBe(0);
  });

  it("moves left and wraps", () => {
    expect(nextTabIndex(0, "ArrowLeft", 5)).toBe(4);
    expect(nextTabIndex(3, "ArrowLeft", 5)).toBe(2);
  });

  it("handles Home and End", () => {
    expect(nextTabIndex(3, "Home", 5)).toBe(0);
    expect(nextTabIndex(1, "End", 5)).toBe(4);
  });

  it("keeps the current index for unrelated keys", () => {
    expect(nextTabIndex(2, "Tab", 5)).toBe(2);
    expect(nextTabIndex(2, "Enter", 5)).toBe(2);
  });

  it("returns 0 for an empty tab list", () => {
    expect(nextTabIndex(0, "ArrowRight", 0)).toBe(0);
  });
});
