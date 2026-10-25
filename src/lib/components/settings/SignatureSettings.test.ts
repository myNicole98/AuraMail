import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import SignatureSettings from "./SignatureSettings.svelte";
import { app } from "../../stores/app.svelte";
import type { Account } from "../../types";

const mocks = vi.hoisted(() => ({ updateAccount: vi.fn() }));
vi.mock("../../api", () => ({ api: { updateAccount: mocks.updateAccount } }));

function account(overrides: Partial<Account> = {}): Account {
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
    created_at: 0,
    ...overrides
  };
}

async function chooseDropdown(label: string, option: string): Promise<void> {
  await fireEvent.click(screen.getByLabelText(label));
  await fireEvent.click(screen.getByRole("option", { name: option }));
}

beforeEach(() => {
  app.spaces = [
    { id: 1, name: "Personal", icon: "layers", position: 0, created_at: 0 },
    { id: 2, name: "Work", icon: "layers", position: 1, created_at: 0 }
  ];
  app.activeSpaceId = 1;
  app.accounts = [
    account({ id: 1, display_name: "Personal", space_id: 1 }),
    account({
      id: 2,
      display_name: "Work account",
      email: "work@example.com",
      space_id: 2
    })
  ];
});

afterEach(() => {
  cleanup();
  app.accounts = [];
  mocks.updateAccount.mockReset();
  localStorage.clear();
});

describe("SignatureSettings", () => {
  it("changes the edited space without changing the active space", async () => {
    render(SignatureSettings);
    await tick();

    expect(screen.getAllByText("Personal").length).toBeGreaterThan(0);
    expect(screen.queryByText("Work account")).not.toBeInTheDocument();

    await chooseDropdown("Edit space", "Work");
    await tick();

    expect(app.activeSpaceId).toBe(1);
    expect(screen.getAllByText("Work account").length).toBeGreaterThan(0);
    expect(screen.queryByText("Personal")).not.toBeInTheDocument();
  });

  it("stores the default-signature toggle on the account", async () => {
    mocks.updateAccount.mockResolvedValue(
      account({ display_name: "Personal", signature_on_new: false })
    );
    render(SignatureSettings);
    await tick();

    await fireEvent.click(
      screen.getByRole("switch", { name: "Add signature by default" })
    );

    await waitFor(() =>
      expect(mocks.updateAccount).toHaveBeenCalledWith(
        1,
        expect.objectContaining({ signature_on_new: false })
      )
    );
    expect(
      screen.getByRole("switch", { name: "Add signature by default" })
    ).toHaveAttribute("aria-checked", "false");
  });
});
