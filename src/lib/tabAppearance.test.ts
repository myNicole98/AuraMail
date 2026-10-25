import { describe, expect, it } from "vitest";
import type { Tab } from "./types";
import {
  TAB_COLORS,
  isHexColor,
  tabColor,
  tabIconId,
  tabIcons,
  tabIconSearch
} from "./tabAppearance";
import { oklabDistance, shiftColor } from "./theme/colorVision";

function tab(overrides: Partial<Tab> = {}): Tab {
  return {
    id: 1,
    space_id: 1,
    name: "Primary",
    description: "Direct mail.",
    position: 0,
    removable: true,
    primary: true,
    color: null,
    icon: null,
    ...overrides
  };
}

function expectDistinctSteps(values: string[]): void {
  for (const value of values) {
    expect(value.slice(1, 3)).toBe(value.slice(3, 5));
    expect(value.slice(3, 5)).toBe(value.slice(5, 7));
  }
  for (let index = 1; index < values.length; index += 1) {
    expect(oklabDistance(values[index - 1], values[index])).toBeGreaterThan(0.05);
  }
}

describe("tab appearance", () => {
  it("validates six-digit hex colours", () => {
    expect(isHexColor("#6da4ff")).toBe(true);
    expect(isHexColor("#ABCDEF")).toBe(true);
    expect(isHexColor("6da4ff")).toBe(false);
    expect(isHexColor("#abc")).toBe(false);
    expect(isHexColor("blue")).toBe(false);
  });

  it("uses the saved colour and falls back by index", () => {
    expect(tabColor(tab({ color: "#e879b9" }), 3)).toBe("#e879b9");
    expect(tabColor(tab({ color: "nope" }), 1)).toBe(TAB_COLORS[1]);
  });

  it("falls back to a folder icon", () => {
    expect(tabIconId(tab({ icon: "bell" }))).toBe("bell");
    expect(tabIconId(tab({ icon: "missing" }))).toBe("folder");
  });

  it("searches the curated icon set by label and id", () => {
    expect(tabIconSearch("bell").map((icon) => icon.id)).toContain("bell");
    expect(tabIconSearch("work").map((icon) => icon.id)).toContain("briefcase");
    expect(tabIconSearch("").length).toBe(tabIcons.length);
  });

  it("separates built-in tab colors in achromatopsia", () => {
    expectDistinctSteps([0, 1, 2, 3, 4, 5, 6, 7].map((index) =>
      tabColor(tab({ color: TAB_COLORS[index] }), index, "achromatopsia", true)
    ));
  });

  it("uses a dark grey ladder for built-in tab colors on light themes", () => {
    expectDistinctSteps([0, 1, 2, 3, 4, 5, 6, 7].map((index) =>
      tabColor(tab({ color: TAB_COLORS[index] }), index, "achromatopsia", false)
    ));
  });

  it("chooses different ladders for each polarity", () => {
    const dark = tabColor(tab({ color: TAB_COLORS[0] }), 0, "achromatopsia", true);
    const light = tabColor(tab({ color: TAB_COLORS[0] }), 0, "achromatopsia", false);
    expect(light).not.toBe(dark);
  });

  it("keys built-in tab colors by palette index in achromatopsia", () => {
    expect(
      tabColor(tab({ color: TAB_COLORS[2].toUpperCase() }), 6, "achromatopsia")
    ).toBe(tabColor(tab({ color: null }), 2, "achromatopsia"));
  });

  it("shifts custom tab colors per color in achromatopsia", () => {
    expect(tabColor(tab({ color: "#123456" }), 0, "achromatopsia")).toBe(
      shiftColor("#123456", "achromatopsia")
    );
  });
});
