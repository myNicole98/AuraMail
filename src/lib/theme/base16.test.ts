import { describe, expect, it } from "vitest";
import { applyBase16, isHexColor, themeIsDark, type Base16 } from "./base16";
import { contrastRatio } from "./semantics";

const palette: Base16 = {
  scheme: "Test",
  colors: {
    base00: "#101010", base01: "#202020", base02: "#303030", base03: "#404040",
    base04: "#505050", base05: "#e0e0e0", base06: "#f0f0f0", base07: "#ffffff",
    base08: "#ff0000", base09: "#ff8800", base0A: "#ffff00", base0B: "#00ff00",
    base0C: "#00ffff", base0D: "#0000ff", base0E: "#ff00ff", base0F: "#884400"
  }
};

describe("Base16", () => {
  it("accepts six-digit hex only", () => {
    expect(isHexColor("#aabbcc")).toBe(true);
    expect(isHexColor("#abc")).toBe(false);
    expect(isHexColor("aabbcc")).toBe(false);
  });

  it("writes every token to the document root", () => {
    applyBase16(palette);
    expect(document.documentElement.style.getPropertyValue("--base00")).toBe("#101010");
    expect(document.documentElement.style.getPropertyValue("--base0F")).toBe("#884400");
    expect(document.documentElement.dataset.themeDark).toBe("true");
    expect(themeIsDark()).toBe(true);
  });

  it("marks light palettes as light", () => {
    applyBase16({ ...palette, colors: { ...palette.colors, base00: "#faf4ed" } });
    expect(document.documentElement.dataset.themeDark).toBe("false");
    expect(themeIsDark()).toBe(false);
  });

  it("applies a readable muted role without changing the base tokens", () => {
    applyBase16(palette);
    const root = document.documentElement;
    const muted = root.style.getPropertyValue("--muted");
    expect(muted).not.toBe(palette.colors.base04);
    expect(contrastRatio(muted, palette.colors.base00)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(muted, palette.colors.base01)).toBeGreaterThanOrEqual(4.5);
    expect(root.style.getPropertyValue("--base04")).toBe(palette.colors.base04);
  });
});
