import { describe, expect, it } from "vitest";
import { insertionIndex } from "./tabOrder";

describe("insertionIndex", () => {
  it("inserts before the first tab whose midpoint is past the pointer", () => {
    expect(insertionIndex(10, [50, 150, 250])).toBe(0);
    expect(insertionIndex(100, [50, 150, 250])).toBe(1);
    expect(insertionIndex(200, [50, 150, 250])).toBe(2);
  });

  it("inserts at the end when the pointer is past every midpoint", () => {
    expect(insertionIndex(400, [50, 150, 250])).toBe(3);
    expect(insertionIndex(0, [])).toBe(0);
  });
});
