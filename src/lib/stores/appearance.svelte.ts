import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  applyAccessibilityPreferences,
  applyThemeSettings,
  currentSystemPrefersDark,
  DEFAULT_UI_SCALE,
  loadPreferences,
  normalizeUiScale,
  uiScale,
  updatePreferences,
  type LayoutMode,
  type MessagePreview
} from "../settings";
import { themeIsDark } from "../theme/base16";
import type { ColorVisionMode } from "../theme/colorVision";
import type { TimeFormat } from "../format";

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

class AppearanceStore {
  uiScale = $state(DEFAULT_UI_SCALE);
  layoutMode = $state<LayoutMode>("classic");
  messagePreview = $state<MessagePreview>("preview");
  reduceMotion = $state(false);
  highContrast = $state(false);
  colorVision = $state<ColorVisionMode>("none");
  timeFormat = $state<TimeFormat>("system");
  themeDark = $state(themeIsDark());
  private zoomTimer: ReturnType<typeof setTimeout> | undefined;
  private themeListenerAttached = false;

  constructor() {
    this.watchTheme();
  }

  async load(): Promise<void> {
    const preferences = loadPreferences();
    this.uiScale = preferences.ui_scale;
    this.layoutMode = preferences.layout_mode;
    this.messagePreview = preferences.message_preview;
    this.reduceMotion = preferences.reduce_motion;
    this.highContrast = preferences.high_contrast;
    this.colorVision = preferences.color_vision;
    this.timeFormat = preferences.time_format;
    this.watchTheme();
    applyAccessibilityPreferences(preferences);
    await this.applyActiveTheme();
    this.themeDark = themeIsDark();
    void this.applyZoom();
  }

  setUiScale(percent: number): void {
    this.uiScale = normalizeUiScale(percent);
    this.persist();
    clearTimeout(this.zoomTimer);
    this.zoomTimer = setTimeout(() => void this.applyZoom(), 40);
  }

  setLayoutMode(mode: LayoutMode): void {
    this.layoutMode = mode;
    this.persist();
  }

  setMessagePreview(mode: MessagePreview): void {
    this.messagePreview = mode;
    this.persist();
  }

  setReduceMotion(value: boolean): void {
    this.reduceMotion = value;
    this.persist();
    this.applyAccessibility();
  }

  setHighContrast(value: boolean): void {
    this.highContrast = value;
    this.persist();
    this.applyAccessibility();
  }

  setColorVision(mode: ColorVisionMode): void {
    this.colorVision = mode;
    this.persist();
    this.applyAccessibility();
    void this.applyActiveTheme();
  }

  setTimeFormat(format: TimeFormat): void {
    this.timeFormat = format;
    this.persist();
  }

  private watchTheme(): void {
    if (this.themeListenerAttached || typeof window === "undefined") return;
    this.themeListenerAttached = true;
    window.addEventListener("auramail:theme-applied", () => {
      this.themeDark = themeIsDark();
    });
  }

  private applyAccessibility(): void {
    applyAccessibilityPreferences({
      ...loadPreferences(),
      reduce_motion: this.reduceMotion,
      high_contrast: this.highContrast,
      color_vision: this.colorVision
    });
  }

  private persist(): void {
    updatePreferences({
      ui_scale: this.uiScale,
      layout_mode: this.layoutMode,
      message_preview: this.messagePreview,
      reduce_motion: this.reduceMotion,
      high_contrast: this.highContrast,
      color_vision: this.colorVision,
      time_format: this.timeFormat
    });
  }

  private async applyActiveTheme(): Promise<void> {
    const preferences = loadPreferences();
    const prefersDark =
      preferences.theme_choice === "system"
        ? await currentSystemPrefersDark()
        : undefined;
    applyThemeSettings(preferences, prefersDark);
  }

  private async applyZoom(): Promise<void> {
    if (!isTauri()) return;
    try {
      await getCurrentWebview().setZoom(uiScale(this.uiScale));
      window.dispatchEvent(new Event("resize"));
    } catch {
      return;
    }
  }
}

export const appearance = new AppearanceStore();
