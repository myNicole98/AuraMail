import { api } from "../api";
import type { DesktopPreferences } from "../types";

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const defaults: DesktopPreferences = {
  notifications: false,
  newMailSound: true,
  sentMailSound: true,
  closeAction: "tray",
  minimizeAction: "taskbar"
};

class DesktopStore {
  notifications = $state(defaults.notifications);
  newMailSound = $state(defaults.newMailSound);
  sentMailSound = $state(defaults.sentMailSound);
  closeAction = $state(defaults.closeAction);
  minimizeAction = $state(defaults.minimizeAction);
  loaded = $state(false);

  private apply(preferences: DesktopPreferences) {
    this.notifications = preferences.notifications;
    this.newMailSound = preferences.newMailSound;
    this.sentMailSound = preferences.sentMailSound;
    this.closeAction = preferences.closeAction;
    this.minimizeAction = preferences.minimizeAction;
    this.loaded = true;
  }

  async load(): Promise<void> {
    if (!isTauri() || this.loaded) return;
    try {
      this.apply(await api.getDesktopPreferences());
    } catch {
      return;
    }
  }

  async update(patch: Partial<DesktopPreferences>): Promise<void> {
    if (!isTauri()) {
      this.apply({ ...this.current(), ...patch });
      return;
    }
    const saved = await api.setDesktopPreferences({
      ...this.current(),
      ...patch
    });
    this.apply(saved);
  }

  private current(): DesktopPreferences {
    return {
      notifications: this.notifications,
      newMailSound: this.newMailSound,
      sentMailSound: this.sentMailSound,
      closeAction: this.closeAction,
      minimizeAction: this.minimizeAction
    };
  }
}

export const desktop = new DesktopStore();
