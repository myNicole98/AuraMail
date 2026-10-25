import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor
} from "@testing-library/svelte";
import ProviderChooser from "./ProviderChooser.svelte";

const mocks = vi.hoisted(() => ({
  googleOAuthStatus: vi.fn(),
  microsoftOAuthStatus: vi.fn()
}));

vi.mock("../../api", () => ({
  api: {
    googleOAuthStatus: mocks.googleOAuthStatus,
    microsoftOAuthStatus: mocks.microsoftOAuthStatus
  }
}));

beforeEach(() => {
  mocks.googleOAuthStatus.mockReset();
  mocks.googleOAuthStatus.mockResolvedValue({ configured: true, reason: null });
  mocks.microsoftOAuthStatus.mockReset();
  mocks.microsoftOAuthStatus.mockResolvedValue({ configured: true, reason: null });
});

afterEach(() => {
  cleanup();
});

describe("ProviderChooser", () => {
  it("disables Google when this build is not configured", async () => {
    mocks.googleOAuthStatus.mockResolvedValue({
      configured: false,
      reason: "Google sign-in is not configured in this build."
    });
    render(ProviderChooser, {
      props: { onGoogle: vi.fn(), onMicrosoft: vi.fn(), onManual: vi.fn() }
    });
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Google" })).toBeDisabled()
    );
    expect(
      screen.getByText("Google sign-in is not configured in this build.")
    ).toBeInTheDocument();
  });

  it("disables Microsoft when this build is not configured", async () => {
    mocks.microsoftOAuthStatus.mockResolvedValue({
      configured: false,
      reason: "Microsoft sign-in is not configured in this build."
    });
    render(ProviderChooser, {
      props: { onGoogle: vi.fn(), onMicrosoft: vi.fn(), onManual: vi.fn() }
    });
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Microsoft" })).toBeDisabled()
    );
    expect(
      screen.getByText("Microsoft sign-in is not configured in this build.")
    ).toBeInTheDocument();
  });

  it("disables Google with a fallback message when the status call fails", async () => {
    mocks.googleOAuthStatus.mockRejectedValue(new Error("boom"));
    render(ProviderChooser, {
      props: { onGoogle: vi.fn(), onMicrosoft: vi.fn(), onManual: vi.fn() }
    });
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Google" })).toBeDisabled()
    );
    expect(screen.getByText("Google sign-in is unavailable.")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "IMAP/SMTP" })
    ).toBeEnabled();
  });

  it("calls onGoogle when Google is configured", async () => {
    const onGoogle = vi.fn();
    mocks.googleOAuthStatus.mockResolvedValue({ configured: true, reason: null });
    render(ProviderChooser, {
      props: { onGoogle, onMicrosoft: vi.fn(), onManual: vi.fn() }
    });
    const google = await screen.findByRole("button", { name: "Google" });
    await waitFor(() => expect(google).toBeEnabled());
    await fireEvent.click(google);
    expect(onGoogle).toHaveBeenCalledTimes(1);
  });

  it("calls onMicrosoft when Microsoft is configured", async () => {
    const onMicrosoft = vi.fn();
    render(ProviderChooser, {
      props: { onGoogle: vi.fn(), onMicrosoft, onManual: vi.fn() }
    });
    const microsoft = await screen.findByRole("button", { name: "Microsoft" });
    await waitFor(() => expect(microsoft).toBeEnabled());
    await fireEvent.click(microsoft);
    expect(onMicrosoft).toHaveBeenCalledTimes(1);
  });

  it("calls onManual for manual setup", async () => {
    const onManual = vi.fn();
    render(ProviderChooser, {
      props: { onGoogle: vi.fn(), onMicrosoft: vi.fn(), onManual }
    });
    await fireEvent.click(
      screen.getByRole("button", { name: "IMAP/SMTP" })
    );
    expect(onManual).toHaveBeenCalledTimes(1);
  });
});
