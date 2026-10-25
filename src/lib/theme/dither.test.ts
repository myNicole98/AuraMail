import { describe, expect, it } from "vitest";
import { applyBayerDither } from "./dither";

describe("applyBayerDither", () => {
  it("splits a half-step value between adjacent 8-bit levels", () => {
    const values = new Set<number>();
    for (let y = 0; y < 8; y += 1) {
      for (let x = 0; x < 8; x += 1) {
        values.add(applyBayerDither(127.5, x, y));
      }
    }
    expect([...values].sort()).toEqual([127, 128]);
  });

  it("keeps values inside the 8-bit range", () => {
    expect(applyBayerDither(-10, 0, 0)).toBe(0);
    expect(applyBayerDither(280, 0, 0)).toBe(255);
  });
});
