import { semanticHover, semanticMuted } from "./semantics";

export const base16Keys = [
  "base00", "base01", "base02", "base03", "base04", "base05", "base06", "base07",
  "base08", "base09", "base0A", "base0B", "base0C", "base0D", "base0E", "base0F"
] as const;

export type Base16Key = (typeof base16Keys)[number];

export type Base16 = {
  scheme: string;
  colors: Record<Base16Key, string>;
};

export function isHexColor(value: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(value);
}

export function validateBase16(value: unknown): Base16 {
  if (!value || typeof value !== "object") throw new Error("Invalid theme");
  const source = value as Record<string, unknown>;
  const colors = {} as Record<Base16Key, string>;
  for (const key of base16Keys) {
    const color = source[key];
    if (typeof color !== "string" || !isHexColor(color)) {
      throw new Error(`Invalid ${key}`);
    }
    colors[key] = color;
  }
  return { scheme: typeof source.scheme === "string" ? source.scheme : "Custom", colors };
}

function isDarkColor(hex: string): boolean {
  const value = Number.parseInt(hex.slice(1), 16);
  const red = (value >> 16) & 0xff;
  const green = (value >> 8) & 0xff;
  const blue = value & 0xff;
  return (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255 < 0.5;
}

export function themeIsDark(): boolean {
  if (typeof document === "undefined") return true;
  return document.documentElement.dataset.themeDark !== "false";
}

export function applyBase16(palette: Base16): void {
  const root = document.documentElement;
  for (const key of base16Keys) root.style.setProperty(`--${key}`, palette.colors[key]);
  root.style.setProperty("--muted", semanticMuted(palette));
  const hover = semanticHover(palette);
  if (hover === palette.colors.base02) {
    root.style.removeProperty("--bg-hover");
  } else {
    root.style.setProperty("--bg-hover", hover);
  }
  root.dataset.themeScheme = palette.scheme;
  root.dataset.themeDark = isDarkColor(palette.colors.base00) ? "true" : "false";
  if (typeof window !== "undefined") {
    window.dispatchEvent(new Event("auramail:theme-applied"));
  }
}
