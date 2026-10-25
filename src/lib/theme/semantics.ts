import type { Base16 } from "./base16";

type Rgb = { r: number; g: number; b: number };

function parseHex(hex: string): Rgb {
  const value = Number.parseInt(hex.slice(1), 16);
  return {
    r: (value >> 16) & 0xff,
    g: (value >> 8) & 0xff,
    b: value & 0xff
  };
}

function toHex(color: Rgb): string {
  return `#${[color.r, color.g, color.b]
    .map((channel) => Math.round(channel).toString(16).padStart(2, "0"))
    .join("")}`;
}

function mix(a: string, b: string, amount: number): string {
  const left = parseHex(a);
  const right = parseHex(b);
  return toHex({
    r: left.r + (right.r - left.r) * amount,
    g: left.g + (right.g - left.g) * amount,
    b: left.b + (right.b - left.b) * amount
  });
}

function luminance(color: Rgb): number {
  const channel = (value: number) => {
    const normalized = value / 255;
    return normalized <= 0.03928
      ? normalized / 12.92
      : ((normalized + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b);
}

export function contrastRatio(a: string, b: string): number {
  const [high, low] = [luminance(parseHex(a)), luminance(parseHex(b))].sort(
    (left, right) => right - left
  );
  return (high + 0.05) / (low + 0.05);
}

function meetsContrast(color: string, surfaces: string[], target = 4.5): boolean {
  return surfaces.every((surface) => contrastRatio(color, surface) >= target);
}

export function semanticHover(palette: Base16): string {
  const { base00, base02, base05 } = palette.colors;
  if (contrastRatio(base05, base02) >= 4.5) return base02;
  for (let step = 1; step <= 20; step += 1) {
    const candidate = mix(base02, base00, step / 20);
    if (contrastRatio(base05, candidate) >= 4.5) return candidate;
  }
  return base00;
}

function mutedSurfaces(palette: Base16): string[] {
  const { base00, base01, base05 } = palette.colors;
  return [
    base00,
    base01,
    mix(base01, base05, 0.05),
    semanticHover(palette)
  ];
}

export function semanticMuted(palette: Base16): string {
  const { base04, base05 } = palette.colors;
  const surfaces = mutedSurfaces(palette);
  if (meetsContrast(base04, surfaces)) return base04;
  for (let step = 1; step <= 20; step += 1) {
    const candidate = mix(base04, base05, step / 20);
    if (meetsContrast(candidate, surfaces)) return candidate;
  }
  return base05;
}
