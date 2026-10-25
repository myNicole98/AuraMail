import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import MessageList from "./MessageList.svelte";
import { app } from "../stores/app.svelte";
import type { Account, Folder, Tab, ThreadMessage } from "../types";

function message(id: number): ThreadMessage {
  return {
    id,
    account_id: 1,
    folder_id: 1,
    uid: id,
    message_id: `<${id}@example.com>`,
    references: null,
    thread_key: `thread-${id}`,
    from_addr: "ada@example.com",
    from_name: "Ada",
    to_addrs: ["me@example.com"],
    cc_addrs: [],
    subject: "Hello",
    date: 100,
    snippet: "Preview",
    is_seen: false,
    is_starred: false,
    has_attachments: false,
    body_cached: true,
    tab_id: 1,
    thread_count: 1,
    thread_unread_count: 1,
    thread_has_inbox: true
  };
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
    auth_type: "password",
    oauth_provider: "",
    space_id: 1,
    position: 0,
    sync_interval_secs: 60,
    created_at: 0
  };
}

function folder(id: number, name: string, role: Folder["role"]): Folder {
  return {
    id,
    account_id: 1,
    path: name,
    name,
    role,
    delimiter: "/",
    uid_validity: 0,
    highest_uid: 0,
    unread_count: 0
  };
}

function tab(id: number, name: string): Tab {
  return {
    id,
    space_id: 1,
    name,
    description: `${name} mail`,
    position: id - 1,
    removable: true,
    primary: id === 1,
    color: "#6da4ff",
    icon: "inbox"
  };
}

beforeEach(() => {
  app.messages = [message(7)];
  app.drafts = [];
  app.spaces = [{ id: 1, name: "Personal", icon: "layers", position: 0, created_at: 0 }];
  app.activeSpaceId = 1;
  app.accounts = [account()];
  app.folders = [folder(1, "Inbox", "inbox"), folder(2, "Archive", "archive")];
  app.tabs = [tab(1, "Primary"), tab(2, "Updates")];
  app.selectedTabId = null;
  app.selectedFolderId = null;
  app.draftsOpen = false;
  app.selectedMessageId = null;
  app.search = "";
  app.loading = false;
  app.classifyingMessageIds = new Set();
  app.flashingMessageIds = new Set();
  app.aiConfigured = true;
  app.classifyBusy = false;
  app.statuses = {};
  app.error = null;
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("MessageList context menu", () => {
  it("opens message actions on right-click", async () => {
    const { container } = render(MessageList);
    const row = container.querySelector(".row");
    expect(row).not.toBeNull();

    await fireEvent.contextMenu(row!);
    await tick();

    expect(screen.getByRole("menu", { name: "Message actions" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Move to folder" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Categorize" })).toBeInTheDocument();
  });

  it("routes a context action to the message", async () => {
    const compose = vi.spyOn(app, "composeMessage").mockResolvedValue(undefined);
    const { container } = render(MessageList);
    await fireEvent.contextMenu(container.querySelector(".row-open")!);
    await tick();

    await fireEvent.click(screen.getByRole("menuitem", { name: "Reply all" }));

    expect(compose).toHaveBeenCalledWith(7, "reply-all");
  });

  it("clears the selection as soon as a bulk action starts", async () => {
    app.messages = [message(7), message(8)];
    let resolveAction = () => {};
    const pending = new Promise<void>((resolve) => {
      resolveAction = resolve;
    });
    vi.spyOn(app, "markMessages").mockReturnValue(pending);
    const { container } = render(MessageList);
    const rows = container.querySelectorAll<HTMLButtonElement>(".row-open");

    await fireEvent.click(rows[0]!, { ctrlKey: true });
    await fireEvent.click(rows[1]!, { ctrlKey: true });
    expect(container.querySelectorAll(".row.checked")).toHaveLength(2);

    await fireEvent.click(screen.getByRole("button", { name: "Mark read" }));
    await tick();

    expect(container.querySelectorAll(".row.checked")).toHaveLength(0);
    resolveAction();
  });

  it("counts every Ctrl-clicked message", async () => {
    app.messages = [message(7), message(8), message(9)];
    const { container } = render(MessageList);
    const rows = container.querySelectorAll<HTMLButtonElement>(".row-open");

    await fireEvent.click(rows[0]!, { ctrlKey: true });
    await fireEvent.click(rows[1]!, { ctrlKey: true });
    await fireEvent.click(rows[2]!, { ctrlKey: true });

    expect(container.querySelectorAll(".row.checked")).toHaveLength(3);
    expect(screen.getByText("3 selected")).toBeInTheDocument();
  });

  it("silently includes the opened mail when extending with Ctrl-click", async () => {
    app.messages = [message(7), message(8)];
    const select = vi
      .spyOn(app, "selectMessage")
      .mockImplementation(async (id: number) => {
        app.selectedMessageId = id;
      });
    const { container } = render(MessageList);
    const buttons = container.querySelectorAll<HTMLButtonElement>(".row-open");

    await fireEvent.click(buttons[0]!);
    expect(select).toHaveBeenCalledWith(7);

    await fireEvent.click(buttons[1]!, { ctrlKey: true });

    expect(container.querySelectorAll(".row.checked")).toHaveLength(2);
    expect(screen.getByText("2 selected")).toBeInTheDocument();
  });

  it("selects a range with Shift-click", async () => {
    app.messages = [message(7), message(8), message(9)];
    const { container } = render(MessageList);
    const rows = container.querySelectorAll<HTMLButtonElement>(".row-open");

    await fireEvent.click(rows[0]!, { ctrlKey: true });
    await fireEvent.click(rows[2]!, { shiftKey: true });

    expect(container.querySelectorAll(".row.checked")).toHaveLength(3);
    expect(screen.getByText("3 selected")).toBeInTheDocument();
    expect(
      container.querySelector(".message-list")?.classList.contains("bulk-selecting")
    ).toBe(true);
  });

  it("clears the bulk selection on the Escape event", async () => {
    app.messages = [message(7), message(8)];
    const { container } = render(MessageList);
    const rows = container.querySelectorAll<HTMLButtonElement>(".row-open");

    await fireEvent.click(rows[0]!, { ctrlKey: true });
    await fireEvent.click(rows[1]!, { ctrlKey: true });
    expect(container.querySelectorAll(".row.checked")).toHaveLength(2);

    const clear = new CustomEvent("auramail:clear-mail-selection", {
      cancelable: true
    });
    window.dispatchEvent(clear);
    await tick();

    expect(clear.defaultPrevented).toBe(true);
    expect(container.querySelectorAll(".row.checked")).toHaveLength(0);
  });

  it("clears the bulk selection when clicking outside", async () => {
    app.messages = [message(7), message(8)];
    const { container } = render(MessageList);
    const rows = container.querySelectorAll<HTMLButtonElement>(".row-open");

    await fireEvent.click(rows[0]!, { ctrlKey: true });
    expect(container.querySelectorAll(".row.checked")).toHaveLength(1);
    expect(
      container.querySelector(".message-list")?.classList.contains("bulk-selecting")
    ).toBe(true);

    await fireEvent.pointerDown(document.body);
    await tick();

    expect(container.querySelectorAll(".row.checked")).toHaveLength(0);
    expect(
      container.querySelector(".message-list")?.classList.contains("bulk-selecting")
    ).toBe(false);
  });

  it("opens a message and clears the selection on a plain click", async () => {
    app.messages = [message(7), message(8)];
    const select = vi.spyOn(app, "selectMessage").mockResolvedValue(undefined);
    const { container } = render(MessageList);
    const rows = container.querySelectorAll<HTMLButtonElement>(".row-open");

    await fireEvent.click(rows[0]!, { ctrlKey: true });
    await fireEvent.click(rows[1]!, { ctrlKey: true });
    await fireEvent.click(rows[0]!);
    await tick();

    expect(select).toHaveBeenCalledWith(7);
    expect(container.querySelectorAll(".row.checked")).toHaveLength(0);
  });
});

describe("MessageList sync banner", () => {
  it("shows a reconnect banner for expired Google authorization", async () => {
    app.statuses = {
      1: { accountId: 1, state: "reauth", message: "Reconnect this account." }
    };
    render(MessageList);
    expect(screen.getByRole("alert")).toHaveTextContent("me@example.com");
    expect(screen.getByRole("button", { name: "Open account settings" }))
      .toBeInTheDocument();
  });

  it("names Microsoft in the reconnect banner", async () => {
    app.accounts = [
      {
        ...account(),
        email: "me@outlook.com",
        auth_type: "oauth",
        oauth_provider: "microsoft"
      }
    ];
    app.statuses = {
      1: { accountId: 1, state: "reauth", message: "Reconnect this account." }
    };
    render(MessageList);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Microsoft authorization expired for me@outlook.com"
    );
  });
});
