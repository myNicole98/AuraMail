import { shiftColor, type ColorVisionMode } from "./theme/colorVision";

export const accountColorTokens = [
  "blue",
  "lavender",
  "mint",
  "peach",
  "rose",
  "sand",
  "sage",
  "sky",
  "lilac",
  "slate"
] as const;

export type AccountColorToken = (typeof accountColorTokens)[number];

const values: Record<string, string> = {
  blue: "#a8c7fa",
  lavender: "#c7b3f2",
  mint: "#b8e0c2",
  peach: "#f7c9a8",
  rose: "#f4b8c1",
  sand: "#f0d9a8",
  sage: "#c7d7b8",
  sky: "#a9e4e8",
  lilac: "#d9c2f0",
  slate: "#c2c9d6"
};

const legacy: Record<string, AccountColorToken> = {
  accent: "blue",
  "accent-alt": "lavender",
  success: "sage",
  warning: "sand",
  info: "sky",
  danger: "rose"
};

const labels: Record<string, string> = {
  blue: "Blue",
  lavender: "Lavender",
  mint: "Mint",
  peach: "Peach",
  rose: "Rose",
  sand: "Sand",
  sage: "Sage",
  sky: "Sky",
  lilac: "Lilac",
  slate: "Slate"
};

const ACHROMATOPSIA_DARK_LADDER = [
  "#dedede",
  "#cccccc",
  "#bbbbbb",
  "#aaaaaa",
  "#999999",
  "#888888",
  "#777777",
  "#666666",
  "#555555",
  "#444444"
];
const ACHROMATOPSIA_LIGHT_LADDER = [
  "#161616",
  "#292929",
  "#3d3d3d",
  "#525252",
  "#696969",
  "#808080",
  "#979797",
  "#aeaeae",
  "#c5c5c5",
  "#dcdcdc"
];

export function isAccountHexColor(value: string): boolean {
  return /^#[0-9a-f]{6}$/i.test(value.trim());
}

export function canonicalAccountColor(value: string): AccountColorToken | string {
  const trimmed = value.trim().toLowerCase();
  return legacy[trimmed] ?? trimmed;
}

export function accountColorValue(
  token: string,
  mode: ColorVisionMode = "none",
  dark = true
): string {
  const custom = token.trim();
  if (isAccountHexColor(custom)) {
    return mode === "achromatopsia"
      ? shiftColor(custom.toLowerCase(), mode)
      : custom.toLowerCase();
  }
  const key = canonicalAccountColor(token);
  if (mode === "achromatopsia") {
    const index = accountColorTokens.indexOf(key as AccountColorToken);
    const ladder = dark
      ? ACHROMATOPSIA_DARK_LADDER
      : ACHROMATOPSIA_LIGHT_LADDER;
    return ladder[index >= 0 ? index : 0];
  }
  const value = values[key] ?? values.blue;
  return shiftColor(value, mode);
}

export function accountColorLabel(token: string): string {
  if (isAccountHexColor(token)) return token.toUpperCase();
  const key = canonicalAccountColor(token);
  return labels[key] ?? labels.blue;
}
