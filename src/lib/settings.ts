import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { TimeFormat } from "./format";
import { applyBase16, validateBase16, type Base16 } from "./theme/base16";
import {
  isThemeChoice,
  normalizeAccent,
  resolveTheme,
  type ThemeChoice
} from "./theme/catalog";
import {
  isColorVisionMode,
  shiftPalette,
  type ColorVisionMode
} from "./theme/colorVision";
import type { ExtraHeader, MessageOrder } from "./types";
import {
  activeSpaceId,
  loadSpaceAppearance,
  loadSpaceGeneral,
  saveSpaceAppearance,
  saveSpaceGeneral
} from "./spaces";

export type { ThemeChoice };
export type ThemeBackground = "solid" | "glass";
export type ThemePanes = "straight" | "rounded";
export const THEME_BACKGROUNDS: { id: ThemeBackground; label: string }[] = [
  { id: "solid", label: "Solid" },
  { id: "glass", label: "Glass" }
];
export const THEME_PANES: { id: ThemePanes; label: string }[] = [
  { id: "straight", label: "Straight" },
  { id: "rounded", label: "Rounded" }
];
export type LayoutMode = "classic" | "reader" | "stacked";
export type MessagePreview = "compact" | "preview";

export const MIN_UI_SCALE = 50;
export const MAX_UI_SCALE = 300;
export const DEFAULT_UI_SCALE = 100;
export const UI_SCALE_LABELS = [50, 100, 150, 200, 250, 300] as const;
export const UI_SCALE_STOPS = [50, 75, 100, 150, 200, 250, 300] as const;
const UI_SCALE_SNAP = 2;

export function normalizeUiScale(value: unknown): number {
  const numeric =
    typeof value === "number"
      ? value
      : typeof value === "string" && value.trim()
        ? Number(value)
        : Number.NaN;
  if (!Number.isFinite(numeric)) return DEFAULT_UI_SCALE;
  return Math.round(Math.min(MAX_UI_SCALE, Math.max(MIN_UI_SCALE, numeric)));
}

export function snapUiScale(value: unknown): number {
  const normalized = normalizeUiScale(value);
  const nearest = UI_SCALE_STOPS.reduce((best, stop) =>
    Math.abs(stop - normalized) < Math.abs(best - normalized) ? stop : best
  );
  return Math.abs(nearest - normalized) <= UI_SCALE_SNAP ? nearest : normalized;
}

export function uiScale(percent: number): number {
  return normalizeUiScale(percent) / 100;
}

export type SettingsPreferences = {
  auto_classify: boolean;
  auto_rail: boolean;
  full_body_context: boolean;
  match_voice: boolean;
  dark_email: boolean;
  always_load_images: boolean;
  profile_pictures: boolean;
  trusted_senders: string[];
  theme_choice: ThemeChoice;
  theme_accent: string;
  theme_background: ThemeBackground;
  theme_panes: ThemePanes;
  ui_scale: number;
  layout_mode: LayoutMode;
  message_preview: MessagePreview;
  reduce_motion: boolean;
  high_contrast: boolean;
  text_spacing: boolean;
  color_vision: ColorVisionMode;
  time_format: TimeFormat;
  mark_read_delay: number;
  message_order: MessageOrder;
  attachment_thumbnails: boolean;
  reveal_download: boolean;
};

export const MARK_READ_DELAYS = [0, 2, 5, 10, 30, -1] as const;

export function normalizeMarkReadDelay(value: unknown): number {
  return typeof value === "number" &&
    (MARK_READ_DELAYS as readonly number[]).includes(value)
    ? value
    : 0;
}

let matugenPalette: Base16 | null = null;

export type AccountForm = {
  display_name: string;
  username: string;
  password: string;
  imap_host: string;
  imap_port: string;
  imap_security: string;
  smtp_host: string;
  smtp_port: string;
  smtp_security: string;
  color: string;
};

const STORAGE_KEY = "auramail.settings";
const HEADER_NAME = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

function presetChoice(value: unknown): ThemeChoice | null {
  if (typeof value !== "string") return null;
  const preset = value.toLowerCase();
  if (preset.startsWith("catppuccin")) return "catppuccin";
  if (preset === "nord") return "nord";
  if (preset.startsWith("gruvbox")) return "gruvbox";
  if (preset === "tokyo-night") return "tokyo-night";
  if (preset === "dracula") return "dracula";
  if (preset.startsWith("rose-pine")) return "rose-pine";
  return null;
}

function legacyThemeChoice(stored: Record<string, unknown>): ThemeChoice {
  if (stored.theme_matugen === true) return "matugen";
  const fromPreset = presetChoice(stored.dark_preset) ?? presetChoice(stored.light_preset);
  if (fromPreset) return fromPreset;
  if (stored.theme_mode === "light" || stored.theme_mode === "dark") {
    return stored.theme_mode;
  }
  return "system";
}

export function defaultPreferences(): SettingsPreferences {
  return {
    auto_classify: false,
    auto_rail: false,
    full_body_context: false,
    match_voice: false,
    dark_email: false,
    always_load_images: false,
    profile_pictures: true,
    trusted_senders: [],
    theme_choice: "system",
    theme_accent: "",
    theme_background: "solid",
    theme_panes: "straight",
    ui_scale: DEFAULT_UI_SCALE,
    layout_mode: "classic",
    message_preview: "preview",
    reduce_motion: false,
    high_contrast: false,
    text_spacing: false,
    color_vision: "none",
    time_format: "system",
    mark_read_delay: 0,
    message_order: "newest",
    attachment_thumbnails: true,
    reveal_download: false
  };
}

function normalizeThemeBackground(value: unknown): ThemeBackground {
  return value === "glass" ? "glass" : "solid";
}

function normalizeThemePanes(value: unknown): ThemePanes {
  return value === "rounded" ? "rounded" : "straight";
}

function legacyThemeShape(style: unknown): {
  background: ThemeBackground;
  panes: ThemePanes;
} {
  if (style === "frosted") return { background: "glass", panes: "straight" };
  if (style === "fancy") return { background: "glass", panes: "rounded" };
  return { background: "solid", panes: "straight" };
}

function storedThemeBackground(stored: Record<string, unknown>): ThemeBackground {
  if (stored.theme_background !== undefined) {
    return normalizeThemeBackground(stored.theme_background);
  }
  return legacyThemeShape(stored.theme_style).background;
}

function storedThemePanes(stored: Record<string, unknown>): ThemePanes {
  if (stored.theme_panes !== undefined) {
    return normalizeThemePanes(stored.theme_panes);
  }
  return legacyThemeShape(stored.theme_style).panes;
}

export function normalizeThemePreferences(
  preferences: SettingsPreferences
): SettingsPreferences {
  const theme_choice = isThemeChoice(preferences.theme_choice)
    ? preferences.theme_choice
    : "system";
  return {
    ...preferences,
    theme_choice,
    theme_accent: normalizeAccent(theme_choice, preferences.theme_accent),
    theme_background: normalizeThemeBackground(preferences.theme_background),
    theme_panes: normalizeThemePanes(preferences.theme_panes)
  };
}

function withSpaceAppearance(
  preferences: SettingsPreferences
): SettingsPreferences {
  const spaceId = activeSpaceId();
  if (spaceId === null) return preferences;
  const appearance = loadSpaceAppearance(spaceId);
  const themed = {
    ...preferences,
    theme_choice: appearance.theme_choice,
    theme_accent: appearance.theme_accent,
    theme_background: appearance.theme_background,
    theme_panes: appearance.theme_panes,
    layout_mode: appearance.layout_mode,
    message_preview: appearance.message_preview,
    dark_email: appearance.dark_email
  };
  const general = loadSpaceGeneral(spaceId);
  return {
    ...themed,
    time_format: general.time_format,
    message_order: general.message_order,
    mark_read_delay: general.mark_read_delay,
    attachment_thumbnails: general.attachment_thumbnails,
    reveal_download: general.reveal_download,
    always_load_images: general.always_load_images,
    profile_pictures: general.profile_pictures,
    trusted_senders: general.trusted_senders
  };
}

export function loadPreferences(): SettingsPreferences {
  const defaults = defaultPreferences();
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return withSpaceAppearance(defaults);
    const stored = JSON.parse(raw) as Record<string, unknown>;
    const theme_choice = isThemeChoice(stored.theme_choice)
      ? stored.theme_choice
      : legacyThemeChoice(stored);
    return withSpaceAppearance({
      auto_classify:
        typeof stored.auto_classify === "boolean"
          ? stored.auto_classify
          : defaults.auto_classify,
      auto_rail:
        typeof stored.auto_rail === "boolean"
          ? stored.auto_rail
          : defaults.auto_rail,
      full_body_context:
        typeof stored.full_body_context === "boolean"
          ? stored.full_body_context
          : defaults.full_body_context,
      match_voice:
        typeof stored.match_voice === "boolean"
          ? stored.match_voice
          : defaults.match_voice,
      dark_email:
        typeof stored.dark_email === "boolean"
          ? stored.dark_email
          : defaults.dark_email,
      always_load_images:
        typeof stored.always_load_images === "boolean"
          ? stored.always_load_images
          : defaults.always_load_images,
      profile_pictures:
        typeof stored.profile_pictures === "boolean"
          ? stored.profile_pictures
          : defaults.profile_pictures,
      trusted_senders: Array.isArray(stored.trusted_senders)
        ? Array.from(
            new Set(
              stored.trusted_senders
                .filter((sender): sender is string => typeof sender === "string")
                .map((sender) => sender.trim().toLowerCase())
                .filter(Boolean)
            )
          )
        : defaults.trusted_senders,
      theme_choice,
      theme_accent: normalizeAccent(theme_choice, stored.theme_accent),
      theme_background: storedThemeBackground(stored),
      theme_panes: storedThemePanes(stored),
      ui_scale:
        stored.ui_scale !== undefined
          ? normalizeUiScale(stored.ui_scale)
          : stored.ui_size === "compact"
            ? 90
            : stored.ui_size === "large"
              ? 110
              : DEFAULT_UI_SCALE,
      layout_mode:
        stored.layout_mode === "classic" ||
        stored.layout_mode === "reader" ||
        stored.layout_mode === "stacked"
          ? stored.layout_mode
          : defaults.layout_mode,
      message_preview:
        stored.message_preview === "compact" ||
        stored.message_preview === "preview"
          ? stored.message_preview
          : defaults.message_preview,
      reduce_motion:
        typeof stored.reduce_motion === "boolean"
          ? stored.reduce_motion
          : defaults.reduce_motion,
      high_contrast:
        typeof stored.high_contrast === "boolean"
          ? stored.high_contrast
          : defaults.high_contrast,
      text_spacing:
        typeof stored.text_spacing === "boolean"
          ? stored.text_spacing
          : defaults.text_spacing,
      color_vision: isColorVisionMode(stored.color_vision)
        ? stored.color_vision
        : defaults.color_vision,
      time_format:
        stored.time_format === "12h" || stored.time_format === "24h"
          ? stored.time_format
          : defaults.time_format,
      mark_read_delay: normalizeMarkReadDelay(stored.mark_read_delay),
      message_order:
        stored.message_order === "oldest" || stored.message_order === "newest"
          ? stored.message_order
          : defaults.message_order,
      attachment_thumbnails:
        typeof stored.attachment_thumbnails === "boolean"
          ? stored.attachment_thumbnails
          : defaults.attachment_thumbnails,
      reveal_download:
        typeof stored.reveal_download === "boolean"
          ? stored.reveal_download
          : defaults.reveal_download
    });
  } catch {
    return defaults;
  }
}

export function savePreferences(preferences: SettingsPreferences): void {
  const normalized = normalizeThemePreferences(preferences);
  localStorage.setItem(STORAGE_KEY, JSON.stringify(normalized));
  const spaceId = activeSpaceId();
  if (spaceId !== null) {
    saveSpaceAppearance(spaceId, {
      theme_choice: normalized.theme_choice,
      theme_accent: normalized.theme_accent,
      theme_background: normalized.theme_background,
      theme_panes: normalized.theme_panes,
      layout_mode: normalized.layout_mode,
      message_preview: normalized.message_preview,
      dark_email: normalized.dark_email
    });
    saveSpaceGeneral(spaceId, {
      ...loadSpaceGeneral(spaceId),
      time_format: normalized.time_format,
      message_order: normalized.message_order,
      mark_read_delay: normalized.mark_read_delay,
      attachment_thumbnails: normalized.attachment_thumbnails,
      reveal_download: normalized.reveal_download,
      always_load_images: normalized.always_load_images,
      profile_pictures: normalized.profile_pictures,
      trusted_senders: normalized.trusted_senders
    });
  }
}

export function updatePreferences(
  patch: Partial<SettingsPreferences>
): SettingsPreferences {
  const next = normalizeThemePreferences({ ...loadPreferences(), ...patch });
  savePreferences(next);
  return next;
}

export function applyAccessibilityPreferences(
  preferences: SettingsPreferences
): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.dataset.reduceMotion = String(preferences.reduce_motion);
  root.dataset.highContrast = String(preferences.high_contrast);
  root.dataset.textSpacing = String(preferences.text_spacing);
  root.dataset.colorVision = preferences.color_vision;
}

export function senderIsTrusted(
  preferences: SettingsPreferences,
  email: string
): boolean {
  const normalized = email.trim().toLowerCase();
  return Boolean(normalized) && preferences.trusted_senders.includes(normalized);
}

export function prefersDarkScheme(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches
  );
}

export async function currentSystemPrefersDark(): Promise<boolean> {
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    try {
      const scheme = await invoke<boolean | null>("system_color_scheme");
      if (typeof scheme === "boolean") return scheme;
    } catch {
      // Fall through to the window theme.
    }
    try {
      const theme = await getCurrentWindow().theme();
      if (theme) return theme === "dark";
    } catch {
      return prefersDarkScheme();
    }
  }
  return prefersDarkScheme();
}

function applyThemeShapeAttributes(
  background: ThemeBackground,
  panes: ThemePanes
): void {
  const root = document.documentElement;
  root.dataset.themeBackground = background;
  root.dataset.themePanes = panes;
  root.dataset.themeGlass = background === "glass" ? "true" : "false";
}

export function applyThemePreferences(
  preferences: SettingsPreferences,
  prefersDark = prefersDarkScheme()
): void {
  const normalized = normalizeThemePreferences(preferences);
  applyThemeShapeAttributes(normalized.theme_background, normalized.theme_panes);
  applyBase16(
    shiftPalette(
      resolveTheme(normalized.theme_choice, prefersDark, normalized.theme_accent),
      normalized.color_vision
    )
  );
}

export function applyThemeSettings(
  preferences: SettingsPreferences,
  prefersDark = prefersDarkScheme()
): void {
  const normalized = normalizeThemePreferences(preferences);
  applyThemeShapeAttributes(normalized.theme_background, normalized.theme_panes);
  if (normalized.theme_choice === "matugen" && matugenPalette) {
    applyBase16(shiftPalette(matugenPalette, normalized.color_vision));
    return;
  }
  applyThemePreferences(normalized, prefersDark);
}

export function applyMatugenPalette(payload: unknown): Base16 {
  const palette = validateBase16(payload);
  // Cache the raw palette; applyThemeSettings re-applies the transform from
  // this raw copy, so painting the shifted palette here never double-shifts.
  matugenPalette = palette;
  applyBase16(shiftPalette(palette, loadPreferences().color_vision));
  return palette;
}

export function clearMatugenPalette(): void {
  matugenPalette = null;
}

export type MatugenReloadResult =
  | { status: "disabled" }
  | { status: "missing" }
  | { status: "invalid"; message: string }
  | { status: "applied"; palette: Base16 };

function themeErrorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Something went wrong";
}

export async function reloadMatugenTheme(
  enabled: boolean,
  load: () => Promise<unknown>,
  preferences: SettingsPreferences,
  prefersDark = prefersDarkScheme()
): Promise<MatugenReloadResult> {
  if (!enabled) return { status: "disabled" };
  try {
    const payload = await load();
    if (payload === null || payload === undefined) {
      applyThemeSettings(preferences, prefersDark);
      return { status: "missing" };
    }
    const palette = applyMatugenPalette(payload);
    applyThemeSettings({ ...preferences, theme_choice: "matugen" }, prefersDark);
    return { status: "applied", palette };
  } catch (error) {
    applyThemeSettings(preferences, prefersDark);
    return { status: "invalid", message: themeErrorMessage(error) };
  }
}

export function watchSystemTheme(onChange: (prefersDark: boolean) => void): () => void {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return () => {};
  }
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const handler = () => onChange(media.matches);
  media.addEventListener("change", handler);
  return () => media.removeEventListener("change", handler);
}

function portProblem(value: string): string | null {
  if (!/^\d+$/.test(value.trim())) return "Enter a port number.";
  const port = Number(value);
  if (port < 1 || port > 65535) return "Port must be between 1 and 65535.";
  return null;
}

export function validateAccountForm(
  form: AccountForm,
  passwordRequired = true
): Record<string, string> {
  const errors: Record<string, string> = {};
  if (!EMAIL.test(form.username.trim())) errors.username = "Enter a valid email address.";
  if (passwordRequired && !form.password) errors.password = "Password is required.";
  if (!form.imap_host.trim()) errors.imap_host = "IMAP host is required.";
  if (!form.smtp_host.trim()) errors.smtp_host = "SMTP host is required.";
  const imapPort = portProblem(form.imap_port);
  if (imapPort) errors.imap_port = imapPort;
  const smtpPort = portProblem(form.smtp_port);
  if (smtpPort) errors.smtp_port = smtpPort;
  if (!["tls", "starttls"].includes(form.imap_security)) {
    errors.imap_security = "Choose TLS or STARTTLS.";
  }
  if (!["tls", "starttls"].includes(form.smtp_security)) {
    errors.smtp_security = "Choose TLS or STARTTLS.";
  }
  return errors;
}

export function validateAiSettings(
  baseUrl: string,
  model: string
): Record<string, string> {
  const errors: Record<string, string> = {};
  const value = baseUrl.trim();
  try {
    const url = new URL(value);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      errors.base_url = "Base URL must start with http:// or https://.";
    }
  } catch {
    errors.base_url = "Base URL must start with http:// or https://.";
  }
  if (!model.trim()) errors.model = "Model is required.";
  return errors;
}

export function parseExtraHeaders(text: string): {
  headers: ExtraHeader[];
  error: string | null;
} {
  const headers: ExtraHeader[] = [];
  const seen = new Set<string>();
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const separator = trimmed.indexOf(":");
    if (separator <= 0) {
      return { headers: [], error: "Write each header as Name: value." };
    }
    const name = trimmed.slice(0, separator).trim();
    const value = trimmed.slice(separator + 1).trim();
    if (!HEADER_NAME.test(name)) {
      return { headers: [], error: `Invalid header name: ${name}` };
    }
    if (!value || /[\r\n]/.test(value)) {
      return { headers: [], error: `Invalid value for header ${name}.` };
    }
    const key = name.toLowerCase();
    if (seen.has(key)) {
      return { headers: [], error: `Duplicate header: ${name}` };
    }
    seen.add(key);
    headers.push([name, value]);
  }
  return { headers, error: null };
}

export function formatExtraHeaders(headers: ExtraHeader[]): string {
  return headers.map(([name, value]) => `${name}: ${value}`).join("\n");
}
