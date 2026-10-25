import type { Base16 } from "./base16";

export type PaletteId =
  | "catppuccin"
  | "nord"
  | "gruvbox"
  | "tokyo-night"
  | "dracula"
  | "one-dark"
  | "solarized"
  | "kanagawa"
  | "rose-pine"
  | "everforest";

export type ThemeChoice = "system" | "light" | "dark" | "matugen" | PaletteId;

export type ThemeAccent = {
  id: string;
  label: string;
  hex: string;
};

export type ThemePalette = {
  id: PaletteId;
  label: string;
  scheme: string;
  colors: Base16["colors"];
  accents: ThemeAccent[];
  defaultAccent: string;
};

export type ThemeCard = {
  id: ThemeChoice;
  label: string;
  kind: "system" | "light" | "dark" | "matugen" | "palette";
  paletteId?: PaletteId;
};

const p = (scheme: string, values: string[]): Base16 => ({
  scheme,
  colors: Object.fromEntries(
    values.map((color, index) => [`base0${index.toString(16).toUpperCase()}`, color])
  ) as Base16["colors"]
});

export const auraLightTheme: Base16 = p("Aura Light", [
  "#F7F8FD", "#EEF1F9", "#E2E7F3", "#C8D0E2",
  "#66708A", "#242A3E", "#141827", "#0A0D17",
  "#D6435B", "#C96A2B", "#9A7412", "#2E8A62",
  "#0E7F97", "#3F60E8", "#6B4EF5", "#A855C8"
]);

export const auraDarkTheme: Base16 = p("Aura Dark", [
  "#0A0A0A", "#121212", "#1A1A1A", "#333333",
  "#8A8A8A", "#E8E8E8", "#F4F4F4", "#FFFFFF",
  "#FF6B81", "#FF9A62", "#F4C95D", "#5ED69B",
  "#4DD6E8", "#7C8DFF", "#A983FF", "#D08CFF"
]);

const catppuccinAccents: ThemeAccent[] = [
  { id: "rosewater", label: "Rosewater", hex: "#f5e0dc" },
  { id: "flamingo", label: "Flamingo", hex: "#f2cdcd" },
  { id: "pink", label: "Pink", hex: "#f5c2e7" },
  { id: "mauve", label: "Mauve", hex: "#cba6f7" },
  { id: "red", label: "Red", hex: "#f38ba8" },
  { id: "maroon", label: "Maroon", hex: "#eba0ac" },
  { id: "peach", label: "Peach", hex: "#fab387" },
  { id: "yellow", label: "Yellow", hex: "#f9e2af" },
  { id: "green", label: "Green", hex: "#a6e3a1" },
  { id: "teal", label: "Teal", hex: "#94e2d5" },
  { id: "sky", label: "Sky", hex: "#89dceb" },
  { id: "sapphire", label: "Sapphire", hex: "#74c7ec" },
  { id: "blue", label: "Blue", hex: "#89b4fa" },
  { id: "lavender", label: "Lavender", hex: "#b4befe" }
];

const solarizedAccents: ThemeAccent[] = [
  { id: "yellow", label: "Yellow", hex: "#b58900" },
  { id: "orange", label: "Orange", hex: "#cb4b16" },
  { id: "red", label: "Red", hex: "#dc322f" },
  { id: "magenta", label: "Magenta", hex: "#d33682" },
  { id: "violet", label: "Violet", hex: "#6c71c4" },
  { id: "blue", label: "Blue", hex: "#268bd2" },
  { id: "cyan", label: "Cyan", hex: "#2aa198" },
  { id: "green", label: "Green", hex: "#859900" }
];

const noAccents: ThemeAccent[] = [];

export const themePalettes: ThemePalette[] = [
  {
    id: "catppuccin",
    label: "Catppuccin",
    scheme: "Catppuccin Mocha",
    accents: catppuccinAccents,
    defaultAccent: "blue",
    colors: p("Catppuccin Mocha", [
      "#1e1e2e", "#181825", "#313244", "#45475a",
      "#585b70", "#cdd6f4", "#f5e0dc", "#b4befe",
      "#f38ba8", "#fab387", "#f9e2af", "#a6e3a1",
      "#94e2d5", "#89b4fa", "#cba6f7", "#f2cdcd"
    ]).colors
  },
  {
    id: "nord",
    label: "Nord",
    scheme: "Nord",
    accents: noAccents,
    defaultAccent: "",
    colors: p("Nord", [
      "#2e3440", "#3b4252", "#434c5e", "#4c566a",
      "#d8dee9", "#e5e9f0", "#eceff4", "#8fbcbb",
      "#bf616a", "#d08770", "#ebcb8b", "#a3be8c",
      "#88c0d0", "#81a1c1", "#b48ead", "#5e81ac"
    ]).colors
  },
  {
    id: "gruvbox",
    label: "Gruvbox",
    scheme: "Gruvbox Dark",
    accents: noAccents,
    defaultAccent: "",
    colors: p("Gruvbox Dark", [
      "#282828", "#3c3836", "#504945", "#665c54",
      "#bdae93", "#d5c4a1", "#ebdbb2", "#fbf1c7",
      "#fb4934", "#fe8019", "#fabd2f", "#b8bb26",
      "#8ec07c", "#83a598", "#d3869b", "#d65d0e"
    ]).colors
  },
  {
    id: "tokyo-night",
    label: "Tokyo Night",
    scheme: "Tokyo Night",
    accents: noAccents,
    defaultAccent: "",
    colors: p("Tokyo Night", [
      "#1a1b26", "#16161e", "#292e42", "#3b4261",
      "#565f89", "#c0caf5", "#d5d6db", "#ffffff",
      "#f7768e", "#ff9e64", "#e0af68", "#9ece6a",
      "#7dcfff", "#7aa2f7", "#bb9af7", "#41a6b5"
    ]).colors
  },
  {
    id: "dracula",
    label: "Dracula",
    scheme: "Dracula",
    accents: noAccents,
    defaultAccent: "",
    colors: p("Dracula", [
      "#282a36", "#21222c", "#343746", "#44475a",
      "#6272a4", "#f8f8f2", "#ffffff", "#ffffff",
      "#ff5555", "#ffb86c", "#f1fa8c", "#50fa7b",
      "#8be9fd", "#bd93f9", "#ff79c6", "#ff92df"
    ]).colors
  },
  {
    id: "one-dark",
    label: "One Dark",
    scheme: "One Dark",
    accents: noAccents,
    defaultAccent: "",
    colors: p("One Dark", [
      "#282c34", "#21252b", "#2c313a", "#3e4451",
      "#5c6370", "#abb2bf", "#c8ccd4", "#ffffff",
      "#e06c75", "#d19a66", "#e5c07b", "#98c379",
      "#56b6c2", "#61afef", "#c678dd", "#be5046"
    ]).colors
  },
  {
    id: "solarized",
    label: "Solarized",
    scheme: "Solarized Dark",
    accents: solarizedAccents,
    defaultAccent: "blue",
    colors: p("Solarized Dark", [
      "#002b36", "#073642", "#586e75", "#657b83",
      "#839496", "#93a1a1", "#eee8d5", "#fdf6e3",
      "#dc322f", "#cb4b16", "#b58900", "#859900",
      "#2aa198", "#268bd2", "#6c71c4", "#d33682"
    ]).colors
  },
  {
    id: "kanagawa",
    label: "Kanagawa",
    scheme: "Kanagawa Wave",
    accents: noAccents,
    defaultAccent: "",
    colors: p("Kanagawa Wave", [
      "#1f1f28", "#181820", "#2a2a37", "#363646",
      "#727169", "#dcd7ba", "#c8c093", "#e6e0c7",
      "#c34043", "#ffa066", "#e6c384", "#98bb6c",
      "#7aa89f", "#7e9cd8", "#957fb8", "#d27e99"
    ]).colors
  },
  {
    id: "rose-pine",
    label: "Rosé Pine",
    scheme: "Rosé Pine Moon",
    accents: noAccents,
    defaultAccent: "",
    colors: p("Rosé Pine Moon", [
      "#232136", "#2a273f", "#393552", "#403d52",
      "#6e6a86", "#e0def4", "#f2e9e1", "#ffffff",
      "#eb6f92", "#ea9a97", "#f6c177", "#3e8fb0",
      "#9ccfd8", "#c4a7e7", "#ebbcba", "#31748f"
    ]).colors
  },
  {
    id: "everforest",
    label: "Everforest",
    scheme: "Everforest Dark",
    accents: noAccents,
    defaultAccent: "",
    colors: p("Everforest Dark", [
      "#2d353b", "#343f44", "#3d484d", "#475258",
      "#859289", "#d3c6aa", "#e6e0c7", "#ffffff",
      "#e67e80", "#e69875", "#dbbc7f", "#a7c080",
      "#83c092", "#7fbbb3", "#d699b6", "#e69875"
    ]).colors
  }
];

export const themeCards: ThemeCard[] = [
  { id: "system", label: "System", kind: "system" },
  { id: "light", label: "Light", kind: "light" },
  { id: "dark", label: "Dark", kind: "dark" },
  { id: "matugen", label: "Matugen", kind: "matugen" },
  ...themePalettes.map((palette) => ({
    id: palette.id,
    label: palette.label,
    kind: "palette" as const,
    paletteId: palette.id
  }))
];

const themeChoices = new Set<ThemeChoice>(themeCards.map((card) => card.id));

export function isThemeChoice(value: unknown): value is ThemeChoice {
  return typeof value === "string" && themeChoices.has(value as ThemeChoice);
}

export function paletteFor(id: PaletteId): ThemePalette {
  return themePalettes.find((palette) => palette.id === id) ?? themePalettes[0];
}

export function accentOptions(choice: ThemeChoice): ThemeAccent[] {
  if (choice === "system" || choice === "light" || choice === "dark" || choice === "matugen") {
    return [];
  }
  return paletteFor(choice).accents;
}

export function defaultAccentFor(choice: ThemeChoice): string {
  if (choice === "system" || choice === "light" || choice === "dark" || choice === "matugen") {
    return "";
  }
  return paletteFor(choice).defaultAccent;
}

export function normalizeAccent(choice: ThemeChoice, accent: unknown): string {
  const options = accentOptions(choice);
  if (options.length === 0) return "";
  return options.some((option) => option.id === accent)
    ? (accent as string)
    : defaultAccentFor(choice);
}

export function resolveTheme(
  choice: ThemeChoice,
  prefersDark: boolean,
  accent = ""
): Base16 {
  if (choice === "system") return prefersDark ? auraDarkTheme : auraLightTheme;
  if (choice === "light") return auraLightTheme;
  if (choice === "dark" || choice === "matugen") return auraDarkTheme;

  const palette = paletteFor(choice);
  const accentId = normalizeAccent(choice, accent);
  const selected = palette.accents.find((option) => option.id === accentId);
  if (!selected) return { scheme: palette.scheme, colors: { ...palette.colors } };
  return {
    scheme: palette.scheme,
    colors: { ...palette.colors, base0D: selected.hex }
  };
}
