import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor
} from "@testing-library/svelte";
import { tick } from "svelte";
import GoogleSignIn from "./GoogleSignIn.svelte";
import type { Account, OAuthUiStatus } from "../../types";

const mocks = vi.hoisted(() => ({
  googleOAuthStatus: vi.fn(),
  startGoogleOAuth: vi.fn(),
  reconnectGoogleAccount: vi.fn(),
  cancelGoogleOAuth: vi.fn(),
  openExternal: vi.fn(),
  listen: vi.fn()
}));

vi.mock("../../api", () => ({
  api: {
    googleOAuthStatus: mocks.googleOAuthStatus,
    startGoogleOAuth: mocks.startGoogleOAuth,
    reconnectGoogleAccount: mocks.reconnectGoogleAccount,
    cancelGoogleOAuth: mocks.cancelGoogleOAuth,
    openExternal: mocks.openExternal
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

type StatusEvent = { payload: OAuthUiStatus };

beforeEach(() => {
  mocks.listen.mockReset();
  mocks.listen.mockResolvedValue(() => {});
  mocks.startGoogleOAuth.mockReset();
  mocks.reconnectGoogleAccount.mockReset();
  mocks.cancelGoogleOAuth.mockReset();
  mocks.openExternal.mockReset();
  mocks.openExternal.mockResolvedValue(undefined);
  mocks.googleOAuthStatus.mockReset();
});

afterEach(() => {
  cleanup();
});

describe("GoogleSignIn", () => {
  it("moves from Waiting for Google… to Connecting…", async () => {
    let callback: ((event: StatusEvent) => void) | undefined;
    mocks.listen.mockImplementation(
      async (_event: string, cb: (event: StatusEvent) => void) => {
        callback = cb;
        return () => {};
      }
    );
    mocks.startGoogleOAuth.mockReturnValue(new Promise<Account>(() => {}));
    render(GoogleSignIn, {
      props: { mode: "add", onCancel: vi.fn(), onConnected: vi.fn() }
    });

    await waitFor(() => expect(mocks.startGoogleOAuth).toHaveBeenCalled());
    expect(screen.getByText("Waiting for Google…")).toBeInTheDocument();

    callback!({ payload: { phase: "connecting" } });
    await tick();
    expect(screen.getByText("Connecting…")).toBeInTheDocument();
  });

  it("calls onConnected on success", async () => {
    const onConnected = vi.fn();
    const acc = account();
    mocks.startGoogleOAuth.mockResolvedValue(acc);
    render(GoogleSignIn, {
      props: { mode: "add", onCancel: vi.fn(), onConnected }
    });
    await waitFor(() => expect(onConnected).toHaveBeenCalledWith(acc));
  });

  it("reconnects an existing account", async () => {
    const onConnected = vi.fn();
    const acc = account();
    mocks.reconnectGoogleAccount.mockResolvedValue(acc);
    render(GoogleSignIn, {
      props: {
        mode: "reconnect",
        account: acc,
        onCancel: vi.fn(),
        onConnected
      }
    });
    await waitFor(() =>
      expect(mocks.reconnectGoogleAccount).toHaveBeenCalledWith(acc.id)
    );
    await waitFor(() => expect(onConnected).toHaveBeenCalledWith(acc));
    expect(mocks.startGoogleOAuth).not.toHaveBeenCalled();
  });

  it("shows the safe message and restarts on Try again", async () => {
    mocks.startGoogleOAuth.mockRejectedValueOnce(
      new Error("Google sign-in failed")
    );
    mocks.startGoogleOAuth.mockResolvedValueOnce(account());
    render(GoogleSignIn, {
      props: { mode: "add", onCancel: vi.fn(), onConnected: vi.fn() }
    });

    expect(await screen.findByText("Google sign-in failed")).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(mocks.startGoogleOAuth).toHaveBeenCalledTimes(2));
  });

  it("offers the authorization link when the browser does not open", async () => {
    let callback: ((event: StatusEvent) => void) | undefined;
    mocks.listen.mockImplementation(
      async (_event: string, cb: (event: StatusEvent) => void) => {
        callback = cb;
        return () => {};
      }
    );
    mocks.startGoogleOAuth.mockReturnValue(new Promise<Account>(() => {}));
    render(GoogleSignIn, {
      props: { mode: "add", onCancel: vi.fn(), onConnected: vi.fn() }
    });

    await waitFor(() => expect(callback).toBeDefined());
    callback!({
      payload: {
        phase: "waiting",
        url: "https://accounts.google.com/o/oauth2/v2/auth?state=test-state"
      }
    });
    await tick();

    expect(screen.getByLabelText("Google sign-in link")).toHaveValue(
      "https://accounts.google.com/o/oauth2/v2/auth?state=test-state"
    );
    expect(
      screen.getByRole("button", { name: "Copy link" })
    ).toBeInTheDocument();
    expect(mocks.openExternal).toHaveBeenCalledWith(
      "https://accounts.google.com/o/oauth2/v2/auth?state=test-state"
    );
  });

  it("focuses Try again when an error replaces the waiting state", async () => {
    mocks.startGoogleOAuth.mockRejectedValueOnce(
      new Error("Google sign-in failed")
    );
    render(GoogleSignIn, {
      props: { mode: "add", onCancel: vi.fn(), onConnected: vi.fn() }
    });

    const retry = await screen.findByRole("button", { name: "Try again" });
    await waitFor(() => expect(document.activeElement).toBe(retry));
  });

  it("cancels the pending flow", async () => {
    const onCancel = vi.fn();
    render(GoogleSignIn, {
      props: { mode: "add", onCancel, onConnected: vi.fn() }
    });
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(mocks.cancelGoogleOAuth).toHaveBeenCalled();
    expect(onCancel).toHaveBeenCalled();
  });

  it("skips the API when reconnecting without an account", async () => {
    render(GoogleSignIn, {
      props: {
        mode: "reconnect",
        account: null,
        onCancel: vi.fn(),
        onConnected: vi.fn()
      }
    });
    expect(
      screen.getByText("Select a Google account to reconnect.")
    ).toBeInTheDocument();
    expect(mocks.reconnectGoogleAccount).not.toHaveBeenCalled();
    expect(mocks.startGoogleOAuth).not.toHaveBeenCalled();
  });
});
