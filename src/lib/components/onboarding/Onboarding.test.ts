import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within
} from "@testing-library/svelte";
import { tick } from "svelte";
import Onboarding from "./Onboarding.svelte";
import { api } from "../../api";
import { ONBOARDING_STORAGE_KEY } from "../../onboarding";
import { loadPreferences } from "../../settings";
import { appearance } from "../../stores/appearance.svelte";
import { shiftPalette } from "../../theme/colorVision";
import { auraLightTheme } from "../../theme/catalog";
import type { Account } from "../../types";

vi.mock("../../api", () => ({
  api: {
    setThemeSettings: vi.fn(),
    discoverAccountSettings: vi.fn(),
    addAccount: vi.fn(),
    googleOAuthStatus: vi.fn(),
    startGoogleOAuth: vi.fn(),
    reconnectGoogleAccount: vi.fn(),
    cancelGoogleOAuth: vi.fn(),
    microsoftOAuthStatus: vi.fn(),
    startMicrosoftOAuth: vi.fn(),
    reconnectMicrosoftAccount: vi.fn(),
    cancelMicrosoftOAuth: vi.fn()
  }
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {})
}));

function googleAccount(overrides: Partial<Account> = {}): Account {
  return {
    id: 1,
    display_name: "Me",
    email: "me@gmail.com",
    username: "me@gmail.com",
    imap_host: "imap.gmail.com",
    imap_port: 993,
    imap_security: "tls",
    smtp_host: "smtp.gmail.com",
    smtp_port: 465,
    smtp_security: "tls",
    color: "#6da4ff",
    signature: "",
    signature_fields: {
      profession: "",
      phone: "",
      location: "",
      website: "",
      socials: []
    },
    signature_on_new: true,
    auth_type: "oauth",
    oauth_provider: "google",
    space_id: 1,
    position: 0,
    sync_interval_secs: 60,
    created_at: 0,
    ...overrides
  };
}

beforeEach(() => {
  localStorage.clear();
  vi.mocked(api.googleOAuthStatus).mockResolvedValue({
    configured: true,
    reason: null
  });
  vi.mocked(api.microsoftOAuthStatus).mockResolvedValue({
    configured: true,
    reason: null
  });
});
afterEach(() => {
  cleanup();
  appearance.setUiScale(100);
  appearance.setLayoutMode("classic");
  appearance.setReduceMotion(false);
  appearance.setHighContrast(false);
  appearance.setColorVision("none");
});

describe("Onboarding shell", () => {
  it("moves from welcome to accessibility to appearance", async () => {
    render(Onboarding, { props: { onComplete: vi.fn() } });

    expect(screen.getByRole("heading", { name: /welcome to auramail/i }))
      .toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();

    expect(screen.getByRole("heading", { name: /make it comfortable/i }))
      .toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();

    expect(screen.getByRole("heading", { name: /make it yours/i }))
      .toBeInTheDocument();
  });

  it("shows six progress segments and the full theme catalog", async () => {
    const { container } = render(Onboarding, { props: { onComplete: vi.fn() } });
    expect(container.querySelectorAll(".progress-segment")).toHaveLength(6);

    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();

    expect(container.querySelectorAll(".theme-grid button")).toHaveLength(14);
  });

  it("offers background and pane styles on the appearance step", async () => {
    render(Onboarding, { props: { onComplete: vi.fn() } });
    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();

    const backgrounds = screen.getByRole("group", { name: "Background" });
    const panes = screen.getByRole("group", { name: "Pane dividers" });
    await fireEvent.click(within(backgrounds).getByRole("button", { name: "Glass" }));
    await waitFor(() =>
      expect(document.documentElement.dataset.themeBackground).toBe("glass")
    );
    await fireEvent.click(within(panes).getByRole("button", { name: "Rounded" }));
    await waitFor(() =>
      expect(document.documentElement.dataset.themePanes).toBe("rounded")
    );

    expect(loadPreferences().theme_background).toBe("glass");
    expect(loadPreferences().theme_panes).toBe("rounded");

    await fireEvent.click(within(backgrounds).getByRole("button", { name: "Solid" }));
    await fireEvent.click(within(panes).getByRole("button", { name: "Straight" }));
    await waitFor(() =>
      expect(document.documentElement.dataset.themeBackground).toBe("solid")
    );
  });

  it("keeps the sidebar in the focus layout preview", async () => {
    const { container } = render(Onboarding, { props: { onComplete: vi.fn() } });
    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Focus" }));
    await tick();

    const preview = container.querySelector(".mail.focus");
    expect(preview).not.toBeNull();
    expect(preview?.querySelector("aside")).not.toBeNull();
    expect(preview?.querySelector("article")).not.toBeNull();
  });

  it("previews text size on input and applies on release", async () => {
    render(Onboarding, { props: { onComplete: vi.fn() } });
    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();

    const slider = screen.getByLabelText("Text size");
    await fireEvent.input(slider, { target: { value: "148" } });
    expect(appearance.uiScale).toBe(100);

    await fireEvent.change(slider, { target: { value: "148" } });
    expect(appearance.uiScale).toBe(150);
  });

  it("offers color vision during accessibility setup", async () => {
    render(Onboarding, { props: { onComplete: vi.fn() } });
    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();

    const select = screen.getByLabelText("Color vision");
    expect(select).toBeInTheDocument();
    await fireEvent.change(select, { target: { value: "protanopia" } });

    expect(loadPreferences().color_vision).toBe("protanopia");
    expect(document.documentElement.dataset.colorVision).toBe("protanopia");
  });

  it("keeps the color-vision choice when picking a theme", async () => {
    render(Onboarding, { props: { onComplete: vi.fn() } });
    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();

    await fireEvent.change(screen.getByLabelText("Color vision"), {
      target: { value: "deuteranopia" }
    });
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();

    await fireEvent.click(screen.getByRole("button", { name: "Light" }));
    await tick();
    await tick();

    expect(loadPreferences().color_vision).toBe("deuteranopia");
    expect(appearance.themeDark).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--base08")).toBe(
      shiftPalette(auraLightTheme, "deuteranopia").colors.base08
    );
  });

  it("offers the AI opt-in before account setup", async () => {
    render(Onboarding, { props: { onComplete: vi.fn() } });
    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();

    expect(screen.getByRole("heading", { name: /let aura help/i })).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Automatic classification" }))
      .toHaveAttribute("aria-checked", "false");
    expect(screen.getByRole("switch", { name: "Full message context" }))
      .toHaveAttribute("aria-checked", "false");
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Skip" })).toBeInTheDocument();

    await fireEvent.click(
      screen.getByRole("switch", { name: "Automatic classification" })
    );
    expect(screen.getByRole("switch", { name: "Automatic classification" }))
      .toHaveAttribute("aria-checked", "true");
    expect(loadPreferences().auto_classify).toBe(true);
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Skip" })).toBeInTheDocument();
  });

  it("opens the provider setup first and includes embedding models", async () => {
    render(Onboarding, { props: { onComplete: vi.fn() } });
    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();

    const provider = screen.getByLabelText("Provider");
    const automatic = screen.getByRole("switch", {
      name: "Automatic classification"
    });
    expect(
      Boolean(
        provider.compareDocumentPosition(automatic) &
          Node.DOCUMENT_POSITION_FOLLOWING
      )
    ).toBe(true);
    expect(screen.getByLabelText("Embedding provider")).toBeInTheDocument();
    expect(screen.getByLabelText("Embedding model")).toBeInTheDocument();
    expect(screen.getByLabelText("Embedding base URL")).toBeInTheDocument();

    await fireEvent.click(provider);
    const providerMenu = screen.getByRole("listbox", { name: "Provider" });
    expect(
      within(providerMenu).getByRole("option", { name: /Ollama/ })
    ).toBeInTheDocument();
    await fireEvent.click(
      within(providerMenu).getByRole("option", { name: /Ollama/ })
    );
    expect(screen.getByLabelText("Provider")).toHaveTextContent("Ollama");
  });

  it("connects an account and reaches ready", async () => {
    const account: Account = {
      id: 1,
      display_name: "Ada",
      email: "ada@example.com",
      username: "ada@example.com",
      imap_host: "imap.example.com",
      imap_port: 993,
      imap_security: "tls",
      smtp_host: "smtp.example.com",
      smtp_port: 465,
      smtp_security: "tls",
      color: "accent",
      signature: "",
      signature_fields: {
        profession: "",
        phone: "",
        location: "",
        website: "",
        socials: []
      },
      signature_on_new: true,
      auth_type: "password",
      oauth_provider: "",
    space_id: 1,
    position: 0,
      sync_interval_secs: 120,
      created_at: 1
    };
    vi.mocked(api.discoverAccountSettings).mockResolvedValue({
      imap_host: "imap.example.com",
      imap_port: 993,
      imap_security: "tls",
      smtp_host: "smtp.example.com",
      smtp_port: 465,
      smtp_security: "tls",
      source: "provider"
    });
    vi.mocked(api.addAccount).mockResolvedValue(account);

    render(Onboarding, { props: { onComplete: vi.fn() } });
    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "IMAP/SMTP" }));
    await tick();

    await fireEvent.input(screen.getByLabelText("Email address"), {
      target: { value: "ada@example.com" }
    });
    await fireEvent.input(screen.getByLabelText("Password"), {
      target: { value: "secret" }
    });
    await fireEvent.input(screen.getByLabelText("Display name"), {
      target: { value: "Ada" }
    });
    await fireEvent.click(screen.getByRole("button", { name: "Connect account" }));
    await tick();
    await tick();

    expect(screen.getByRole("heading", { name: /you.?re all set/i }))
      .toBeInTheDocument();
    expect(screen.getByText("ada@example.com")).toBeInTheDocument();
  });

  it("keeps the account form on verification failure", async () => {
    vi.mocked(api.discoverAccountSettings).mockResolvedValue({
      imap_host: "imap.example.com",
      imap_port: 993,
      imap_security: "tls",
      smtp_host: "smtp.example.com",
      smtp_port: 465,
      smtp_security: "tls",
      source: "provider"
    });
    vi.mocked(api.addAccount).mockRejectedValue("Authentication failed");

    render(Onboarding, { props: { onComplete: vi.fn() } });
    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "IMAP/SMTP" }));
    await tick();

    await fireEvent.input(screen.getByLabelText("Email address"), {
      target: { value: "ada@example.com" }
    });
    await fireEvent.input(screen.getByLabelText("Password"), {
      target: { value: "secret" }
    });
    await fireEvent.input(screen.getByLabelText("Display name"), {
      target: { value: "Ada" }
    });
    await fireEvent.click(screen.getByRole("button", { name: "Connect account" }));
    await tick();
    await tick();

    expect(screen.getByRole("alert")).toHaveTextContent("Authentication failed");
    expect(screen.getByLabelText("Email address")).toHaveValue("ada@example.com");
  });

  it("skips account setup and completes onboarding", async () => {
    const onComplete = vi.fn();
    render(Onboarding, { props: { onComplete } });

    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Set up later" }));
    await tick();

    expect(screen.getByRole("heading", { name: /you.?re all set/i }))
      .toBeInTheDocument();
    expect(screen.getByText(/add an account anytime in settings/i)).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: "Open AuraMail" }));
    expect(onComplete).toHaveBeenCalledTimes(1);
    expect(localStorage.getItem(ONBOARDING_STORAGE_KEY)).toContain('"completed":true');
  });

  it("offers Google, Microsoft, and manual setup on the account step", async () => {
    render(Onboarding, { props: { onComplete: vi.fn() } });
    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    await tick();

    expect(screen.getByRole("button", { name: "Google" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Microsoft" }))
      .toBeInTheDocument();
    expect(screen.getByRole("button", { name: "IMAP/SMTP" }))
      .toBeInTheDocument();
  });

  it("connects a Google account from onboarding", async () => {
    vi.mocked(api.startGoogleOAuth).mockResolvedValue(googleAccount());
    render(Onboarding, { props: { onComplete: vi.fn() } });
    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    await tick();

    const google = screen.getByRole("button", { name: "Google" });
    await waitFor(() => expect(google).toBeEnabled());
    await fireEvent.click(google);
    await waitFor(() => expect(api.startGoogleOAuth).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.getByText("Account added and verified.")).toBeInTheDocument()
    );
  });

  it("connects a Microsoft account from onboarding", async () => {
    vi.mocked(api.startMicrosoftOAuth).mockResolvedValue(
      googleAccount({
        email: "me@outlook.com",
        username: "me@outlook.com",
        oauth_provider: "microsoft"
      })
    );
    render(Onboarding, { props: { onComplete: vi.fn() } });
    await fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    await tick();

    const microsoft = screen.getByRole("button", { name: "Microsoft" });
    await waitFor(() => expect(microsoft).toBeEnabled());
    await fireEvent.click(microsoft);
    await waitFor(() => expect(api.startMicrosoftOAuth).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.getByText("Account added and verified.")).toBeInTheDocument()
    );
  });
});
