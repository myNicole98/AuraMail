import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor
} from "@testing-library/svelte";
import AddAccountDialog from "./AddAccountDialog.svelte";
import type { Account } from "../../types";

const mocks = vi.hoisted(() => ({
  googleOAuthStatus: vi.fn(),
  startGoogleOAuth: vi.fn(),
  reconnectGoogleAccount: vi.fn(),
  cancelGoogleOAuth: vi.fn(),
  microsoftOAuthStatus: vi.fn(),
  startMicrosoftOAuth: vi.fn(),
  reconnectMicrosoftAccount: vi.fn(),
  cancelMicrosoftOAuth: vi.fn(),
  discoverAccountSettings: vi.fn(),
  addAccount: vi.fn(),
  listen: vi.fn()
}));

vi.mock("../../api", () => ({
  api: {
    googleOAuthStatus: mocks.googleOAuthStatus,
    startGoogleOAuth: mocks.startGoogleOAuth,
    reconnectGoogleAccount: mocks.reconnectGoogleAccount,
    cancelGoogleOAuth: mocks.cancelGoogleOAuth,
    microsoftOAuthStatus: mocks.microsoftOAuthStatus,
    startMicrosoftOAuth: mocks.startMicrosoftOAuth,
    reconnectMicrosoftAccount: mocks.reconnectMicrosoftAccount,
    cancelMicrosoftOAuth: mocks.cancelMicrosoftOAuth,
    discoverAccountSettings: mocks.discoverAccountSettings,
    addAccount: mocks.addAccount
  }
}));

vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));

function account(overrides: Partial<Account> = {}): Account {
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
  mocks.listen.mockReset();
  mocks.listen.mockResolvedValue(() => {});
  mocks.googleOAuthStatus.mockReset();
  mocks.googleOAuthStatus.mockResolvedValue({ configured: true, reason: null });
  mocks.startGoogleOAuth.mockReset();
  mocks.startGoogleOAuth.mockResolvedValue(account());
  mocks.reconnectGoogleAccount.mockReset();
  mocks.cancelGoogleOAuth.mockReset();
  mocks.microsoftOAuthStatus.mockReset();
  mocks.microsoftOAuthStatus.mockResolvedValue({ configured: true, reason: null });
  mocks.startMicrosoftOAuth.mockReset();
  mocks.startMicrosoftOAuth.mockResolvedValue(
    account({ email: "me@outlook.com", oauth_provider: "microsoft" })
  );
  mocks.reconnectMicrosoftAccount.mockReset();
  mocks.cancelMicrosoftOAuth.mockReset();
  mocks.discoverAccountSettings.mockReset();
  mocks.addAccount.mockReset();
});

afterEach(() => {
  cleanup();
});

describe("AddAccountDialog", () => {
  it("keeps the IMAP/SMTP form inside the add dialog", async () => {
    render(AddAccountDialog, {
      props: {
        mode: "add",
        spaceId: 1,
        account: null,
        onClose: vi.fn(),
        onConnected: vi.fn()
      }
    });
    await fireEvent.click(screen.getByRole("button", { name: "IMAP/SMTP" }));
    expect(screen.getByRole("dialog", { name: "Add account" })).toBeInTheDocument();
    expect(screen.getByLabelText("IMAP host")).toBeInTheDocument();
    expect(screen.queryByText(/Manual IMAP/)).not.toBeInTheDocument();
  });

  it("autodetects IMAP and SMTP settings inside the dialog", async () => {
    mocks.discoverAccountSettings.mockResolvedValue({
      imap_host: "imap.example.com",
      imap_port: 993,
      imap_security: "tls",
      smtp_host: "smtp.example.com",
      smtp_port: 465,
      smtp_security: "tls",
      source: "known"
    });
    render(AddAccountDialog, {
      props: {
        mode: "add",
        spaceId: 1,
        account: null,
        onClose: vi.fn(),
        onConnected: vi.fn()
      }
    });
    await fireEvent.click(screen.getByRole("button", { name: "IMAP/SMTP" }));
    await fireEvent.input(screen.getByLabelText("Email"), {
      target: { value: "me@example.com" }
    });
    await fireEvent.click(screen.getByRole("button", { name: "Autodetect" }));
    await waitFor(() =>
      expect(mocks.discoverAccountSettings).toHaveBeenCalledWith("me@example.com")
    );
    expect(screen.getByLabelText("IMAP host")).toHaveValue("imap.example.com");
    expect(screen.getByLabelText("SMTP host")).toHaveValue("smtp.example.com");
  });

  it("shows the Google flow after choosing Google in add mode", async () => {
    render(AddAccountDialog, {
      props: {
        mode: "add",
        account: null,
        onClose: vi.fn(),
        onConnected: vi.fn()
      }
    });
    const google = await screen.findByRole("button", { name: "Google" });
    await fireEvent.click(google);
    expect(await screen.findByText("Waiting for Google…")).toBeInTheDocument();
  });

  it("shows the Microsoft flow after choosing Microsoft in add mode", async () => {
    render(AddAccountDialog, {
      props: {
        mode: "add",
        account: null,
        onClose: vi.fn(),
        onConnected: vi.fn()
      }
    });
    const microsoft = await screen.findByRole("button", { name: "Microsoft" });
    await fireEvent.click(microsoft);
    expect(await screen.findByText("Waiting for Microsoft…")).toBeInTheDocument();
  });

  it("goes straight to Google in reconnect mode", async () => {
    render(AddAccountDialog, {
      props: {
        mode: "reconnect",
        account: account(),
        onClose: vi.fn(),
        onConnected: vi.fn()
      }
    });
    expect(await screen.findByText("Waiting for Google…")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "IMAP/SMTP" })
    ).not.toBeInTheDocument();
  });

  it("goes straight to Microsoft in reconnect mode", async () => {
    render(AddAccountDialog, {
      props: {
        mode: "reconnect",
        account: account({
          email: "me@outlook.com",
          oauth_provider: "microsoft"
        }),
        onClose: vi.fn(),
        onConnected: vi.fn()
      }
    });
    expect(await screen.findByText("Waiting for Microsoft…")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "IMAP/SMTP" })
    ).not.toBeInTheDocument();
  });

  it("closes on Escape without letting it reach the window", async () => {
    const onClose = vi.fn();
    const windowHandler = vi.fn();
    document.addEventListener("keydown", windowHandler);
    render(AddAccountDialog, {
      props: {
        mode: "add",
        account: null,
        onClose,
        onConnected: vi.fn()
      }
    });
    const dialog = screen.getByRole("dialog");
    await fireEvent.keyDown(dialog, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(windowHandler).not.toHaveBeenCalled();
    document.removeEventListener("keydown", windowHandler);
  });

  it("closes on backdrop click", async () => {
    const onClose = vi.fn();
    const { container } = render(AddAccountDialog, {
      props: {
        mode: "add",
        account: null,
        onClose,
        onConnected: vi.fn()
      }
    });
    const backdrop = container.querySelector(".dialog-backdrop");
    expect(backdrop).not.toBeNull();
    await fireEvent.click(backdrop!);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("moves focus inside the panel after choosing Google", async () => {
    render(AddAccountDialog, {
      props: {
        mode: "add",
        account: null,
        onClose: vi.fn(),
        onConnected: vi.fn()
      }
    });
    await fireEvent.click(await screen.findByRole("button", { name: "Google" }));
    await waitFor(() =>
      expect(screen.getByText("Waiting for Google…")).toBeInTheDocument()
    );
    const panel = screen.getByRole("dialog");
    await waitFor(() =>
      expect(panel.contains(document.activeElement)).toBe(true)
    );
  });

  it("keeps Escape scoped to the dialog after switching to Google", async () => {
    const onClose = vi.fn();
    const windowHandler = vi.fn();
    document.addEventListener("keydown", windowHandler);
    render(AddAccountDialog, {
      props: {
        mode: "add",
        account: null,
        onClose,
        onConnected: vi.fn()
      }
    });
    await fireEvent.click(await screen.findByRole("button", { name: "Google" }));
    const cancel = await screen.findByRole("button", { name: "Cancel" });
    await waitFor(() => expect(document.activeElement).toBe(cancel));
    await fireEvent.keyDown(cancel, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(windowHandler).not.toHaveBeenCalled();
    document.removeEventListener("keydown", windowHandler);
  });

  it("focuses the panel when reconnect has no account", async () => {
    render(AddAccountDialog, {
      props: {
        mode: "reconnect",
        account: null,
        onClose: vi.fn(),
        onConnected: vi.fn()
      }
    });
    const panel = screen.getByRole("dialog");
    await waitFor(() => expect(document.activeElement).toBe(panel));
  });
});
