import { describe, expect, it } from "vitest";
import type { Base16 } from "./base16";
import {
  contrastRatio,
  semanticHover,
  semanticMuted
} from "./semantics";
import {
  auraDarkTheme,
  auraLightTheme,
  themePalettes
} from "./catalog";

function selectedSurface(palette: Base16): string {
  const background = palette.colors.base01;
  const text = palette.colors.base05;
  return mixHex(background, text, 0.05);
}

function mixHex(a: string, b: string, amount: number): string {
  const parse = (hex: string) =>
    [1, 3, 5].map((offset) => Number.parseInt(hex.slice(offset, offset + 2), 16));
  const left = parse(a);
  const right = parse(b);
  return `#${left
    .map((channel, index) =>
      Math.round(channel + (right[index] - channel) * amount)
        .toString(16)
        .padStart(2, "0")
    )
    .join("")}`;
}

const palettes: [string, Base16][] = [
  ["Aura Dark", auraDarkTheme],
  ["Aura Light", auraLightTheme],
  ...themePalettes.map((palette) => [palette.label, palette] as [string, Base16])
];

describe("theme semantics", () => {
  it("keeps muted text readable on every palette surface", () => {
    for (const [name, palette] of palettes) {
      const muted = semanticMuted(palette);
      const hover = semanticHover(palette);
      for (const surface of [
        palette.colors.base00,
        palette.colors.base01,
        selectedSurface(palette),
        hover
      ]) {
        expect(contrastRatio(muted, surface), `${name} ${muted} on ${surface}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("only changes hover when the canonical hover tone cannot carry text", () => {
    const catppuccin = themePalettes.find((palette) => palette.id === "catppuccin");
    const solarized = themePalettes.find((palette) => palette.id === "solarized");
    expect(catppuccin).toBeDefined();
    expect(solarized).toBeDefined();
    expect(semanticHover(catppuccin!)).toBe(catppuccin!.colors.base02);
    expect(semanticHover(solarized!)).not.toBe(solarized!.colors.base02);
    expect(contrastRatio(solarized!.colors.base05, semanticHover(solarized!))).toBeGreaterThanOrEqual(4.5);
  });

  it("keeps the canonical muted tone when it already passes", () => {
    expect(semanticMuted(auraDarkTheme)).toBe(auraDarkTheme.colors.base04);
  });
});
