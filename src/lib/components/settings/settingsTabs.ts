export type SettingsSection =
  | "general"
  | "account"
  | "appearance"
  | "accessibility"
  | "signatures"
  | "ai";

export type SettingsTab = {
  id: SettingsSection;
  label: string;
};

export const SETTINGS_TABS: SettingsTab[] = [
  { id: "general", label: "General" },
  { id: "account", label: "Account" },
  { id: "appearance", label: "Appearance" },
  { id: "accessibility", label: "Accessibility" },
  { id: "signatures", label: "Signatures" },
  { id: "ai", label: "Aura AI" }
];

export function nextTabIndex(current: number, key: string, count: number): number {
  if (count <= 0) return 0;
  switch (key) {
    case "ArrowRight":
      return (current + 1) % count;
    case "ArrowLeft":
      return (current - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return current;
  }
}
