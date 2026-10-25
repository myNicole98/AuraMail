import { describe, expect, it } from "vitest";
import { auraDarkTheme, auraLightTheme } from "./catalog";
import {
  COLOR_VISION_MODES,
  isColorVisionMode,
  oklabDistance,
  shiftColor,
  shiftPalette,
  simulateColorVision
} from "./colorVision";

const HEX = /^#[0-9a-f]{6}$/i;
const chromatic = [
  "base08", "base09", "base0A", "base0B",
  "base0C", "base0D", "base0E", "base0F"
] as const;
const neutral = [
  "base00", "base01", "base02", "base03",
  "base04", "base05", "base06", "base07"
] as const;
const pairs = [
  ["base08", "base0B"],
  ["base09", "base0C"],
  ["base0A", "base0D"],
  ["base0E", "base0F"]
] as const;

describe("color vision modes", () => {
  it("exposes the five modes and validates values", () => {
    expect(COLOR_VISION_MODES.map((mode) => mode.id)).toEqual([
      "none", "protanopia", "deuteranopia", "tritanopia", "achromatopsia"
    ]);
    expect(isColorVisionMode("deuteranopia")).toBe(true);
    expect(isColorVisionMode("unknown")).toBe(false);
    expect(isColorVisionMode(null)).toBe(false);
  });

  it("returns the input unchanged for none", () => {
    expect(shiftPalette(auraDarkTheme, "none")).toEqual(auraDarkTheme);
    expect(shiftColor("#e879b9", "none")).toBe("#e879b9");
  });

  it("keeps neutrals and emits valid hex for every palette mode", () => {
    for (const mode of COLOR_VISION_MODES.map((item) => item.id)) {
      for (const palette of [auraDarkTheme, auraLightTheme]) {
        const shifted = shiftPalette(palette, mode);
        expect(shifted.scheme).toBe(palette.scheme);
        for (const key of chromatic) expect(shifted.colors[key]).toMatch(HEX);
        if (mode !== "achromatopsia") {
          for (const key of neutral) {
            expect(shifted.colors[key]).toBe(palette.colors[key]);
          }
        }
      }
    }
  });

  it("keeps semantic pairs distinguishable after simulation", () => {
    for (const mode of ["protanopia", "deuteranopia", "tritanopia"] as const) {
      for (const palette of [auraDarkTheme, auraLightTheme]) {
        const shifted = shiftPalette(palette, mode);
        for (const [left, right] of pairs) {
          const distance = oklabDistance(
            simulateColorVision(shifted.colors[left], mode),
            simulateColorVision(shifted.colors[right], mode)
          );
          expect(distance, `${mode} ${left}/${right}`).toBeGreaterThan(0.08);
        }
      }
    }
  });

  it("desaturates and separates luminance for achromatopsia", () => {
    const shifted = shiftPalette(auraDarkTheme, "achromatopsia");
    for (const key of chromatic) {
      const value = shifted.colors[key];
      expect(value.slice(1, 3)).toBe(value.slice(3, 5));
      expect(value.slice(3, 5)).toBe(value.slice(5, 7));
    }
    for (let index = 1; index < chromatic.length; index += 1) {
      const previous = shifted.colors[chromatic[index - 1]];
      const current = shifted.colors[chromatic[index]];
      expect(oklabDistance(previous, current)).toBeGreaterThan(0.05);
    }
  });

  it("shifts an arbitrary color and tolerates invalid input", () => {
    expect(shiftColor("#e879b9", "deuteranopia")).toMatch(HEX);
    expect(shiftColor("not-a-color", "tritanopia")).toBe("not-a-color");
  });
});
