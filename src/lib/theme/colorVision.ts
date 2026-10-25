import type { Base16, Base16Key } from "./base16";

export type ColorVisionMode =
  | "none"
  | "protanopia"
  | "deuteranopia"
  | "tritanopia"
  | "achromatopsia";

export const COLOR_VISION_MODES: readonly { id: ColorVisionMode; label: string }[] = [
  { id: "none", label: "None" },
  { id: "protanopia", label: "Protanopia" },
  { id: "deuteranopia", label: "Deuteranopia" },
  { id: "tritanopia", label: "Tritanopia" },
  { id: "achromatopsia", label: "Achromatopsia" }
];

const CHROMATIC_KEYS: readonly Base16Key[] = [
  "base08", "base09", "base0A", "base0B",
  "base0C", "base0D", "base0E", "base0F"
];

const NEUTRAL_KEYS: readonly Base16Key[] = [
  "base00", "base01", "base02", "base03",
  "base04", "base05", "base06", "base07"
];

// Families pair chromatic slots that should stay distinguishable from each other.
const FAMILIES: readonly (readonly Base16Key[])[] = [
  ["base08", "base09", "base0A", "base0F"],
  ["base0B", "base0C", "base0D", "base0E"]
];

const ACHROMATOPSIA_DARK_LUMINANCE = [0.90, 0.835, 0.77, 0.705, 0.64, 0.575, 0.51, 0.445];
const ACHROMATOPSIA_LIGHT_LUMINANCE = [0.12, 0.18, 0.24, 0.30, 0.36, 0.42, 0.48, 0.54];

const FAMILY_DARK_LUMINANCE = [0.60, 0.70, 0.80, 0.50];
const FAMILY_LIGHT_LUMINANCE = [0.42, 0.52, 0.62, 0.32];

const FAMILY_CHROMA = 0.12;

const FAMILY_HUES: Record<"protanopia" | "deuteranopia" | "tritanopia", readonly [number, number]> = {
  protanopia: [85, 250],
  deuteranopia: [85, 250],
  tritanopia: [25, 150]
};

const HEX_PATTERN = /^#[0-9a-f]{6}$/i;

function clamp01(value: number): number {
  return Math.min(1, Math.max(0, value));
}

function srgbToLinear(channel: number): number {
  return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
}

function linearToSrgb(channel: number): number {
  const value = clamp01(channel);
  return value <= 0.0031308 ? value * 12.92 : 1.055 * value ** (1 / 2.4) - 0.055;
}

function parseHex(hex: string): [number, number, number] | null {
  if (!HEX_PATTERN.test(hex)) return null;
  const value = Number.parseInt(hex.slice(1), 16);
  return [
    srgbToLinear(((value >> 16) & 0xff) / 255),
    srgbToLinear(((value >> 8) & 0xff) / 255),
    srgbToLinear((value & 0xff) / 255)
  ];
}

function formatHex(linear: readonly [number, number, number]): string {
  const channels = linear.map((channel) => {
    const byte = Math.round(linearToSrgb(channel) * 255);
    return byte.toString(16).padStart(2, "0");
  });
  return `#${channels.join("")}`;
}

type Oklab = readonly [number, number, number];

function linearToOklab(linear: readonly [number, number, number]): Oklab {
  const [red, green, blue] = linear;
  const l = Math.cbrt(0.4122214708 * red + 0.5363325363 * green + 0.0514459929 * blue);
  const m = Math.cbrt(0.2119034982 * red + 0.6806995451 * green + 0.1073969566 * blue);
  const s = Math.cbrt(0.0883024619 * red + 0.2817188376 * green + 0.6299787005 * blue);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s
  ];
}

function oklabToLinear([lightness, a, b]: Oklab): [number, number, number] {
  const l = (lightness + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (lightness - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (lightness - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s
  ];
}

function oklabToOklch([lightness, a, b]: Oklab): [number, number, number] {
  const chroma = Math.hypot(a, b);
  let hue = (Math.atan2(b, a) * 180) / Math.PI;
  if (hue < 0) hue += 360;
  return [lightness, chroma, hue];
}

function oklchToOklab(lightness: number, chroma: number, hue: number): Oklab {
  const radians = (hue * Math.PI) / 180;
  return [lightness, chroma * Math.cos(radians), chroma * Math.sin(radians)];
}

function isInGamut(linear: readonly [number, number, number]): boolean {
  const epsilon = 1e-6;
  return linear.every((channel) => channel >= -epsilon && channel <= 1 + epsilon);
}

/** Converts OKLCH to hex, lowering chroma until the color fits in sRGB. */
function oklchToHex(lightness: number, chroma: number, hue: number): string {
  const level = clamp01(lightness);
  const targetChroma = Math.max(0, chroma);

  if (targetChroma < 1e-6) {
    const gray = level ** 3;
    const byte = Math.round(linearToSrgb(gray) * 255).toString(16).padStart(2, "0");
    return `#${byte}${byte}${byte}`;
  }

  let low = 0;
  let high = targetChroma;
  let best = oklabToLinear(oklchToOklab(level, 0, hue));

  if (isInGamut(oklabToLinear(oklchToOklab(level, targetChroma, hue)))) {
    return formatHex(oklabToLinear(oklchToOklab(level, targetChroma, hue)));
  }

  for (let step = 0; step < 24; step += 1) {
    const candidate = (low + high) / 2;
    const linear = oklabToLinear(oklchToOklab(level, candidate, hue));
    if (isInGamut(linear)) {
      low = candidate;
      best = linear;
    } else {
      high = candidate;
    }
  }

  return formatHex(best);
}

function hexToOklch(hex: string): [number, number, number] | null {
  const linear = parseHex(hex);
  return linear ? oklabToOklch(linearToOklab(linear)) : null;
}

function hueDistance(a: number, b: number): number {
  return Math.abs(((a - b + 540) % 360) - 180);
}

function nearestHue(hue: number, options: readonly number[]): number {
  return options.reduce((closest, candidate) =>
    hueDistance(hue, candidate) < hueDistance(hue, closest) ? candidate : closest
  );
}

function isDarkPalette(palette: Base16): boolean {
  const oklch = hexToOklch(palette.colors.base00);
  return oklch !== null && oklch[0] < 0.5;
}

export function isColorVisionMode(value: unknown): value is ColorVisionMode {
  return (
    typeof value === "string" &&
    COLOR_VISION_MODES.some((mode) => mode.id === value)
  );
}

export function shiftColor(hex: string, mode: ColorVisionMode): string {
  if (mode === "none") return hex;
  const oklch = hexToOklch(hex);
  if (!oklch) return hex;

  const [lightness, chroma, hue] = oklch;
  if (mode === "achromatopsia") return oklchToHex(lightness, 0, 0);

  const safeHue = nearestHue(hue, FAMILY_HUES[mode]);
  return oklchToHex(lightness, Math.min(chroma, FAMILY_CHROMA), safeHue);
}

export function shiftPalette(palette: Base16, mode: ColorVisionMode): Base16 {
  const colors = { ...palette.colors };
  if (mode === "none") return { scheme: palette.scheme, colors };

  const dark = isDarkPalette(palette);

  if (mode === "achromatopsia") {
    const ladder = dark ? ACHROMATOPSIA_DARK_LUMINANCE : ACHROMATOPSIA_LIGHT_LUMINANCE;
    CHROMATIC_KEYS.forEach((key, index) => {
      colors[key] = oklchToHex(ladder[index], 0, 0);
    });
    for (const key of NEUTRAL_KEYS) {
      const oklch = hexToOklch(palette.colors[key]);
      colors[key] = oklch ? oklchToHex(oklch[0], 0, 0) : palette.colors[key];
    }
    return { scheme: palette.scheme, colors };
  }

  const ladder = dark ? FAMILY_DARK_LUMINANCE : FAMILY_LIGHT_LUMINANCE;
  const hues = FAMILY_HUES[mode];
  FAMILIES.forEach((keys, familyIndex) => {
    keys.forEach((key, index) => {
      colors[key] = oklchToHex(ladder[index], FAMILY_CHROMA, hues[familyIndex]);
    });
  });

  return { scheme: palette.scheme, colors };
}

export function simulateColorVision(hex: string, mode: ColorVisionMode): string {
  if (mode === "none") return hex;
  const linear = parseHex(hex);
  if (!linear) return hex;

  if (mode === "achromatopsia") {
    const oklch = hexToOklch(hex);
    return oklch ? oklchToHex(oklch[0], 0, 0) : hex;
  }

  const matrix = SIMULATION_MATRICES[mode];
  const [red, green, blue] = linear;
  const simulated: [number, number, number] = [
    matrix[0] * red + matrix[1] * green + matrix[2] * blue,
    matrix[3] * red + matrix[4] * green + matrix[5] * blue,
    matrix[6] * red + matrix[7] * green + matrix[8] * blue
  ];
  return formatHex(simulated);
}

export function oklabDistance(a: string, b: string): number {
  const left = parseHex(a);
  const right = parseHex(b);
  if (!left || !right) return 0;
  const first = linearToOklab(left);
  const second = linearToOklab(right);
  return Math.hypot(
    first[0] - second[0],
    first[1] - second[1],
    first[2] - second[2]
  );
}

// Machado et al. (2009), severity 1.0, applied to linear sRGB.
const SIMULATION_MATRICES: Record<
  "protanopia" | "deuteranopia" | "tritanopia",
  readonly number[]
> = {
  protanopia: [
    0.152286, 1.052583, -0.204868,
    0.114503, 0.786281, 0.099216,
    -0.003882, -0.048116, 1.051998
  ],
  deuteranopia: [
    0.367322, 0.860646, -0.227968,
    0.280085, 0.672501, 0.047413,
    -0.01182, 0.04294, 0.968881
  ],
  tritanopia: [
    1.255528, -0.076749, -0.178779,
    -0.078411, 0.930809, 0.147602,
    0.004733, 0.691367, 0.3039
  ]
};
