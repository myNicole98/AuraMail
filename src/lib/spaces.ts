import type { ThemeChoice } from "./theme/catalog";
import type { SpaceAppearance, SpaceGeneral, SpaceWorkflow } from "./types";
import type { LayoutMode, MessagePreview } from "./settings";
import type { ThemeBackground, ThemePanes } from "./settings";

const ACTIVE_SPACE_KEY = "auramail.activeSpaceId";
const LEGACY_SETTINGS_KEY = "auramail.settings";
const GENERAL_MIGRATED_KEY = "auramail.space.general.migrated";

export function defaultSpaceAppearance(): SpaceAppearance {
  return {
    theme_choice: "system",
    theme_accent: "",
    theme_background: "solid",
    theme_panes: "straight",
    layout_mode: "classic",
    message_preview: "preview",
    dark_email: false
  };
}

export function defaultSpaceGeneral(): SpaceGeneral {
  return {
    time_format: "system",
    message_order: "newest",
    mark_read_delay: 0,
    attachment_thumbnails: true,
    reveal_download: false,
    always_load_images: false,
    profile_pictures: true,
    trusted_senders: []
  };
}

export function defaultSpaceWorkflow(): SpaceWorkflow {
  return {
    selectedFolderId: null,
    selectedTabId: null,
    selectedMessageId: null,
    unified: "inbox",
    draftsOpen: false
  };
}

function storage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

function appearanceKey(spaceId: number): string {
  return `auramail.space.${spaceId}.appearance`;
}

function workflowKey(spaceId: number): string {
  return `auramail.space.${spaceId}.workflow`;
}

function generalKey(spaceId: number): string {
  return `auramail.space.${spaceId}.general`;
}

function bool(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function themeChoice(value: unknown): ThemeChoice {
  return typeof value === "string"
    ? (value as ThemeChoice)
    : defaultSpaceAppearance().theme_choice;
}

function themeBackground(value: unknown): ThemeBackground {
  return value === "glass" ? "glass" : "solid";
}

function themePanes(value: unknown): ThemePanes {
  return value === "rounded" ? "rounded" : "straight";
}

function layoutMode(value: unknown): LayoutMode {
  return value === "reader" || value === "stacked" ? value : "classic";
}

function messagePreview(value: unknown): MessagePreview {
  return value === "compact" ? "compact" : "preview";
}

export function normalizeSpaceAppearance(value: unknown): SpaceAppearance {
  const defaults = defaultSpaceAppearance();
  if (!value || typeof value !== "object") return defaults;
  const stored = value as Record<string, unknown>;
  return {
    theme_choice: themeChoice(stored.theme_choice),
    theme_accent:
      typeof stored.theme_accent === "string"
        ? stored.theme_accent
        : defaults.theme_accent,
    theme_background: themeBackground(stored.theme_background),
    theme_panes: themePanes(stored.theme_panes),
    layout_mode: layoutMode(stored.layout_mode),
    message_preview: messagePreview(stored.message_preview),
    dark_email: bool(stored.dark_email, defaults.dark_email)
  };
}

export function loadSpaceAppearance(
  spaceId: number | null | undefined
): SpaceAppearance {
  const store = storage();
  if (!store || spaceId == null) return defaultSpaceAppearance();
  try {
    const raw = store.getItem(appearanceKey(spaceId));
    return raw
      ? normalizeSpaceAppearance(JSON.parse(raw))
      : defaultSpaceAppearance();
  } catch {
    return defaultSpaceAppearance();
  }
}

export function saveSpaceAppearance(
  spaceId: number,
  appearance: SpaceAppearance
): void {
  storage()?.setItem(
    appearanceKey(spaceId),
    JSON.stringify(normalizeSpaceAppearance(appearance))
  );
}

export function normalizeSpaceWorkflow(value: unknown): SpaceWorkflow {
  const defaults = defaultSpaceWorkflow();
  if (!value || typeof value !== "object") return defaults;
  const stored = value as Record<string, unknown>;
  const unified =
    stored.unified === "sent" ||
    stored.unified === "archive" ||
    stored.unified === "spam" ||
    stored.unified === "trash" ||
    stored.unified === "starred"
      ? stored.unified
      : "inbox";
  const id = (candidate: unknown): number | null =>
    typeof candidate === "number" && Number.isFinite(candidate)
      ? candidate
      : null;
  return {
    selectedFolderId: id(stored.selectedFolderId),
    selectedTabId: id(stored.selectedTabId),
    selectedMessageId: id(stored.selectedMessageId),
    unified,
    draftsOpen: bool(stored.draftsOpen, defaults.draftsOpen)
  };
}

export function loadSpaceWorkflow(
  spaceId: number | null | undefined
): SpaceWorkflow {
  const store = storage();
  if (!store || spaceId == null) return defaultSpaceWorkflow();
  try {
    const raw = store.getItem(workflowKey(spaceId));
    return raw
      ? normalizeSpaceWorkflow(JSON.parse(raw))
      : defaultSpaceWorkflow();
  } catch {
    return defaultSpaceWorkflow();
  }
}

export function saveSpaceWorkflow(
  spaceId: number,
  workflow: SpaceWorkflow
): void {
  storage()?.setItem(
    workflowKey(spaceId),
    JSON.stringify(normalizeSpaceWorkflow(workflow))
  );
}

export function normalizeSpaceGeneral(value: unknown): SpaceGeneral {
  const defaults = defaultSpaceGeneral();
  if (!value || typeof value !== "object") return defaults;
  const stored = value as Record<string, unknown>;
  const delay =
    typeof stored.mark_read_delay === "number" &&
    [0, 2, 5, 10, 30, -1].includes(stored.mark_read_delay)
      ? stored.mark_read_delay
      : defaults.mark_read_delay;
  const trusted = Array.isArray(stored.trusted_senders)
    ? Array.from(
        new Set(
          stored.trusted_senders
            .filter((sender): sender is string => typeof sender === "string")
            .map((sender) => sender.trim().toLowerCase())
            .filter(Boolean)
        )
      )
    : defaults.trusted_senders;
  return {
    time_format:
      stored.time_format === "12h" || stored.time_format === "24h"
        ? stored.time_format
        : "system",
    message_order: stored.message_order === "oldest" ? "oldest" : "newest",
    mark_read_delay: delay,
    attachment_thumbnails: bool(
      stored.attachment_thumbnails,
      defaults.attachment_thumbnails
    ),
    reveal_download: bool(stored.reveal_download, defaults.reveal_download),
    always_load_images: bool(
      stored.always_load_images,
      defaults.always_load_images
    ),
    profile_pictures: bool(
      stored.profile_pictures,
      defaults.profile_pictures
    ),
    trusted_senders: trusted
  };
}

export function loadSpaceGeneral(
  spaceId: number | null | undefined
): SpaceGeneral {
  const store = storage();
  if (!store || spaceId == null) return defaultSpaceGeneral();
  try {
    const raw = store.getItem(generalKey(spaceId));
    return raw
      ? normalizeSpaceGeneral(JSON.parse(raw))
      : defaultSpaceGeneral();
  } catch {
    return defaultSpaceGeneral();
  }
}

export function hasSpaceGeneral(spaceId: number): boolean {
  return storage()?.getItem(generalKey(spaceId)) != null;
}

export function saveSpaceGeneral(
  spaceId: number,
  general: SpaceGeneral
): void {
  storage()?.setItem(
    generalKey(spaceId),
    JSON.stringify(normalizeSpaceGeneral(general))
  );
}

export function activeSpaceId(): number | null {
  const raw = storage()?.getItem(ACTIVE_SPACE_KEY);
  if (!raw) return null;
  const value = Number(raw);
  return Number.isFinite(value) && value > 0 ? value : null;
}

export function setActiveSpaceId(spaceId: number | null): void {
  if (spaceId == null) storage()?.removeItem(ACTIVE_SPACE_KEY);
  else storage()?.setItem(ACTIVE_SPACE_KEY, String(spaceId));
}

/**
 * Copies the pre-spaces appearance fields out of the legacy global settings
 * blob once, when the default space's appearance has not been written yet.
 */
export function migrateLegacyAppearance(defaultSpaceId: number): void {
  const store = storage();
  if (!store) return;
  if (store.getItem(appearanceKey(defaultSpaceId)) !== null) return;
  try {
    const raw = store.getItem(LEGACY_SETTINGS_KEY);
    if (!raw) return;
    const legacy = JSON.parse(raw) as Record<string, unknown>;
    const defaults = defaultSpaceAppearance();
    saveSpaceAppearance(defaultSpaceId, {
      theme_choice:
        typeof legacy.theme_choice === "string"
          ? (legacy.theme_choice as ThemeChoice)
          : defaults.theme_choice,
      theme_accent:
        typeof legacy.theme_accent === "string"
          ? legacy.theme_accent
          : defaults.theme_accent,
      theme_background: themeBackground(legacy.theme_background),
      theme_panes: themePanes(legacy.theme_panes),
      layout_mode: layoutMode(legacy.layout_mode),
      message_preview: messagePreview(legacy.message_preview),
      dark_email: bool(legacy.dark_email, defaults.dark_email)
    });
  } catch {
    // Ignore corrupt legacy settings; defaults are safe.
  }
}

/**
 * Copies the pre-spaces General fields out of the legacy global settings blob
 * once, when the default space's General settings have not been written yet.
 */
export function migrateLegacyGeneral(
  defaultSpaceId: number,
  spaceIds: number[] = [defaultSpaceId]
): void {
  const store = storage();
  if (!store) return;
  if (store.getItem(GENERAL_MIGRATED_KEY) !== null) return;
  try {
    const raw = store.getItem(LEGACY_SETTINGS_KEY);
    if (raw) {
      const general = normalizeSpaceGeneral(JSON.parse(raw));
      for (const spaceId of spaceIds) {
        if (store.getItem(generalKey(spaceId)) === null) {
          saveSpaceGeneral(spaceId, general);
        }
      }
    }
  } catch {
    // Ignore corrupt legacy settings; defaults are safe.
  } finally {
    store.setItem(GENERAL_MIGRATED_KEY, "1");
  }
}
