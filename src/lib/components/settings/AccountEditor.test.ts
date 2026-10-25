import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import AccountEditor from "./AccountEditor.svelte";
import type { Account, Space } from "../../types";

const mocks = vi.hoisted(() => ({
  setAccountColor: vi.fn(),
  reconnectMicrosoftAccount: vi.fn()
}));
vi.mock("../../api", () => ({
  api: {
    setAccountColor: mocks.setAccountColor,
    reconnectMicrosoftAccount: mocks.reconnectMicrosoftAccount
  }
}));

function space(id: number, name: string): Space {
  return { id, name, icon: "layers", position: id - 1, created_at: 0 };
}

function account(): Account {
  return {
    id: 1,
    display_name: "Me",
    email: "me@example.com",
    username: "me@example.com",
    imap_host: "imap.example.com",
    imap_port: 993,
    imap_security: "tls",
    smtp_host: "smtp.example.com",
    smtp_port: 465,
    smtp_security: "tls",
    color: "blue",
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
    sync_interval_secs: 60,
    created_at: 0
  };
}

beforeEach(() => {
  mocks.setAccountColor.mockReset();
  mocks.reconnectMicrosoftAccount.mockReset();
});

afterEach(cleanup);

describe("AccountEditor", () => {
  it("leads with personalization and collapses account setup", async () => {
    render(AccountEditor, {
      props: {
        account: account(),
        spaces: [space(1, "Personal")],
        onSaved: vi.fn(),
        onRemoved: vi.fn(),
        onColorChanged: vi.fn()
      }
    });
    expect(screen.getByLabelText("Display name")).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Color" })).toBeInTheDocument();
    expect(screen.getByLabelText("Custom color")).toBeInTheDocument();
    expect(screen.queryByLabelText("Custom color hex")).not.toBeInTheDocument();
    await fireEvent.click(screen.getByLabelText("Custom color"));
    expect(screen.getByLabelText("Custom color hex")).toBeInTheDocument();
    expect(screen.getByLabelText("Pick custom color")).toBeInTheDocument();
    expect(screen.getByLabelText("Space")).toBeInTheDocument();
    expect(
      screen.queryByText(/In the active space|Moves on save/)
    ).not.toBeInTheDocument();
    expect(screen.queryByLabelText("IMAP host")).not.toBeInTheDocument();
    await fireEvent.click(screen.getByText("Account setup"));
    expect(screen.getByLabelText("IMAP host")).toBeInTheDocument();
  });

  it("saves a preset color immediately", async () => {
    const onColorChanged = vi.fn();
    mocks.setAccountColor.mockResolvedValue({ ...account(), color: "mint" });
    render(AccountEditor, {
      props: {
        account: account(),
        spaces: [space(1, "Personal")],
        onSaved: vi.fn(),
        onRemoved: vi.fn(),
        onColorChanged
      }
    });

    await fireEvent.click(screen.getByRole("button", { name: "Mint" }));

    await waitFor(() =>
      expect(mocks.setAccountColor).toHaveBeenCalledWith(1, "mint")
    );
    expect(onColorChanged).toHaveBeenCalled();
  });

  it("uses the Microsoft provider badge and reconnect path", async () => {
    const microsoft = {
      ...account(),
      email: "me@outlook.com",
      auth_type: "oauth" as const,
      oauth_provider: "microsoft"
    };
    mocks.reconnectMicrosoftAccount.mockResolvedValue(microsoft);
    const onSaved = vi.fn();
    render(AccountEditor, {
      props: {
        account: microsoft,
        spaces: [space(1, "Personal")],
        onSaved,
        onRemoved: vi.fn(),
        onColorChanged: vi.fn()
      }
    });

    expect(screen.getByText("Microsoft")).toBeInTheDocument();
    await fireEvent.click(screen.getByText("Account setup"));
    await fireEvent.click(
      screen.getByRole("button", { name: "Reconnect Microsoft" })
    );

    await waitFor(() =>
      expect(mocks.reconnectMicrosoftAccount).toHaveBeenCalledWith(1)
    );
    expect(onSaved).toHaveBeenCalledWith(microsoft);
    expect(
      await screen.findByText("Microsoft account reconnected.")
    ).toBeInTheDocument();
  });
});
