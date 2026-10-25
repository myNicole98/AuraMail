import { describe, expect, it } from "vitest";
import { base16Keys, isHexColor, type Base16 } from "./base16";
import {
  accentOptions,
  auraDarkTheme,
  auraLightTheme,
  defaultAccentFor,
  isThemeChoice,
  normalizeAccent,
  resolveTheme,
  themeCards,
  themePalettes,
  type ThemePalette
} from "./catalog";

const expectValidPalette = (palette: Base16 | ThemePalette) => {
  expect(palette.scheme.trim()).not.toBe("");
  for (const key of base16Keys) {
    expect(isHexColor(palette.colors[key]), `${palette.scheme} ${key}`).toBe(true);
  }
};

describe("theme catalog", () => {
  it("orders the theme cards as specified", () => {
    expect(themeCards.map((card) => card.id)).toEqual([
      "system",
      "light",
      "dark",
      "matugen",
      "catppuccin",
      "nord",
      "gruvbox",
      "tokyo-night",
      "dracula",
      "one-dark",
      "solarized",
      "kanagawa",
      "rose-pine",
      "everforest"
    ]);
  });

  it("uses the palette names as card labels", () => {
    expect(themeCards.slice(4).map((card) => card.label)).toEqual([
      "Catppuccin",
      "Nord",
      "Gruvbox",
      "Tokyo Night",
      "Dracula",
      "One Dark",
      "Solarized",
      "Kanagawa",
      "Rosé Pine",
      "Everforest"
    ]);
  });

  it("provides a valid Base16 palette for every choice", () => {
    for (const palette of [auraLightTheme, auraDarkTheme, ...themePalettes]) {
      expectValidPalette(palette);
    }
  });

  it("uses the Aura palettes for system, light, and dark", () => {
    expect(resolveTheme("system", true)).toEqual(auraDarkTheme);
    expect(resolveTheme("system", false)).toEqual(auraLightTheme);
    expect(resolveTheme("light", true)).toEqual(auraLightTheme);
    expect(resolveTheme("dark", false)).toEqual(auraDarkTheme);
  });

  it("uses neutral surfaces for the default dark palette", () => {
    for (const key of ["base00", "base01", "base02", "base03", "base04", "base05", "base06", "base07"] as const) {
      const hex = auraDarkTheme.colors[key];
      expect(hex.slice(1, 3), `${key} red`).toBe(hex.slice(3, 5));
      expect(hex.slice(3, 5), `${key} green`).toBe(hex.slice(5, 7));
    }
  });

  it("uses each palette's canonical dark flavor", () => {
    expect(resolveTheme("catppuccin", false).scheme).toBe("Catppuccin Mocha");
    expect(resolveTheme("nord", false).scheme).toBe("Nord");
    expect(resolveTheme("gruvbox", false).scheme).toBe("Gruvbox Dark");
    expect(resolveTheme("tokyo-night", false).scheme).toBe("Tokyo Night");
    expect(resolveTheme("dracula", false).scheme).toBe("Dracula");
    expect(resolveTheme("one-dark", false).scheme).toBe("One Dark");
    expect(resolveTheme("solarized", false).scheme).toBe("Solarized Dark");
    expect(resolveTheme("kanagawa", false).scheme).toBe("Kanagawa Wave");
    expect(resolveTheme("rose-pine", false).scheme).toBe("Rosé Pine Moon");
    expect(resolveTheme("everforest", false).scheme).toBe("Everforest Dark");
  });

  it("exposes only documented accent sets", () => {
    expect(accentOptions("catppuccin")).toHaveLength(14);
    expect(accentOptions("solarized")).toHaveLength(8);
    expect(accentOptions("nord")).toEqual([]);
    expect(accentOptions("system")).toEqual([]);
  });

  it("overrides only the primary accent", () => {
    const base = resolveTheme("catppuccin", false);
    const red = resolveTheme("catppuccin", false, "red");
    expect(red.colors.base0D).toBe("#f38ba8");
    expect(red.colors.base0E).toBe(base.colors.base0E);
    for (const key of base16Keys.filter((token) => token !== "base0D")) {
      expect(red.colors[key]).toBe(base.colors[key]);
    }
  });

  it("falls back to the default accent for unknown or wrong-family values", () => {
    expect(normalizeAccent("catppuccin", "red")).toBe("red");
    expect(normalizeAccent("catppuccin", "mauve")).toBe("mauve");
    expect(normalizeAccent("catppuccin", "nope")).toBe(defaultAccentFor("catppuccin"));
    expect(normalizeAccent("nord", "red")).toBe(defaultAccentFor("nord"));
    expect(normalizeAccent("system", "red")).toBe("");
  });

  it("recognizes every card choice", () => {
    for (const card of themeCards) expect(isThemeChoice(card.id)).toBe(true);
    expect(isThemeChoice("unknown")).toBe(false);
  });
});
