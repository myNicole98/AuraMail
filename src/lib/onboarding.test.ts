import { beforeEach, describe, expect, it } from "vitest";
import {
  ONBOARDING_STEPS,
  ONBOARDING_STORAGE_KEY,
  markOnboardingComplete,
  onboardingCompleted,
  shouldMigrateOnboarding,
  shouldShowOnboarding
} from "./onboarding";

beforeEach(() => localStorage.clear());

describe("onboarding store", () => {
  it("defines five ordered steps", () => {
    expect(ONBOARDING_STEPS.map((step) => step.id)).toEqual([
      "welcome",
      "accessibility",
      "appearance",
      "ai",
      "account",
      "ready"
    ]);
  });

  it("writes and reads a completion marker", () => {
    expect(onboardingCompleted()).toBe(false);
    markOnboardingComplete(new Date("2026-10-20T12:00:00.000Z"));
    expect(onboardingCompleted()).toBe(true);
    expect(JSON.parse(localStorage.getItem(ONBOARDING_STORAGE_KEY)!)).toEqual({
      completed: true,
      completedAt: "2026-10-20T12:00:00.000Z"
    });
  });

  it("treats malformed storage as incomplete", () => {
    localStorage.setItem(ONBOARDING_STORAGE_KEY, "{broken");
    expect(onboardingCompleted()).toBe(false);
  });

  it("decides when onboarding is due", () => {
    expect(
      shouldShowOnboarding({ booted: true, accountCount: 0, completed: false })
    ).toBe(true);
    expect(
      shouldShowOnboarding({ booted: false, accountCount: 0, completed: false })
    ).toBe(false);
    expect(
      shouldShowOnboarding({ booted: true, accountCount: 1, completed: false })
    ).toBe(false);
    expect(
      shouldShowOnboarding({ booted: true, accountCount: 0, completed: true })
    ).toBe(false);
  });

  it("migrates existing accounts without a marker", () => {
    expect(
      shouldMigrateOnboarding({ booted: true, accountCount: 1, completed: false })
    ).toBe(true);
    expect(
      shouldMigrateOnboarding({ booted: true, accountCount: 0, completed: false })
    ).toBe(false);
    expect(
      shouldMigrateOnboarding({ booted: true, accountCount: 1, completed: true })
    ).toBe(false);
  });
});
