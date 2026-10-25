export const ONBOARDING_STORAGE_KEY = "auramail.onboarding.v1";

export const ONBOARDING_STEPS = [
  { id: "welcome", label: "Welcome" },
  { id: "accessibility", label: "Accessibility" },
  { id: "appearance", label: "Appearance" },
  { id: "ai", label: "Aura AI" },
  { id: "account", label: "Account" },
  { id: "ready", label: "Ready" }
] as const;

export type OnboardingStep = (typeof ONBOARDING_STEPS)[number]["id"];

type StorageLike = Pick<Storage, "getItem" | "setItem">;

function storage(): StorageLike | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

export function onboardingCompleted(store = storage()): boolean {
  if (!store) return false;
  try {
    const raw = store.getItem(ONBOARDING_STORAGE_KEY);
    if (!raw) return false;
    const value = JSON.parse(raw) as { completed?: unknown };
    return value.completed === true;
  } catch {
    return false;
  }
}

export function markOnboardingComplete(
  now = new Date(),
  store = storage()
): void {
  if (!store) return;
  store.setItem(
    ONBOARDING_STORAGE_KEY,
    JSON.stringify({ completed: true, completedAt: now.toISOString() })
  );
}

export function shouldShowOnboarding(input: {
  booted: boolean;
  accountCount: number;
  completed: boolean;
}): boolean {
  return input.booted && input.accountCount === 0 && !input.completed;
}

export function shouldMigrateOnboarding(input: {
  booted: boolean;
  accountCount: number;
  completed: boolean;
}): boolean {
  return input.booted && input.accountCount > 0 && !input.completed;
}
