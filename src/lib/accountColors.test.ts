import { describe, expect, it } from "vitest";
import { accountColorTokens, accountColorValue } from "./accountColors";
import { oklabDistance } from "./theme/colorVision";

function ladder(dark: boolean): string[] {
  return accountColorTokens.map((token) =>
    accountColorValue(token, "achromatopsia", dark)
  );
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

describe("account colors", () => {
  it("uses fixed pastel colors and maps legacy tokens", () => {
    expect(accountColorValue("blue", "none")).toBe("#a8c7fa");
    expect(accountColorValue("accent", "none")).toBe("#a8c7fa");
    expect(accountColorValue("accent-alt", "none")).toBe("#c7b3f2");
    expect(accountColorValue("blue", "deuteranopia")).not.toBe("#a8c7fa");
  });

  it("uses a light grey ladder on dark themes", () => {
    expectDistinctSteps(ladder(true));
  });

  it("uses a dark grey ladder on light themes", () => {
    expectDistinctSteps(ladder(false));
  });

  it("preserves custom hex colors", () => {
    expect(accountColorValue("#12ABEF", "none")).toBe("#12abef");
    expect(accountColorValue("#12ABEF", "achromatopsia")).not.toBe("#12abef");
  });

  it("chooses different ladders for each polarity", () => {
    const dark = ladder(true);
    const light = ladder(false);
    expect(light[0]).not.toBe(dark[0]);
    expect(light[light.length - 1]).not.toBe(dark[dark.length - 1]);
  });
});
