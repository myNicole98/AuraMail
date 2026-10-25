import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import MailContextMenu from "./MailContextMenu.svelte";
import type { Folder, Tab, ThreadMessage } from "../types";

function message(): ThreadMessage {
  return {
    id: 7,
    account_id: 1,
    folder_id: 1,
    uid: 7,
    message_id: "<7@example.com>",
    references: null,
    thread_key: "thread-7",
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

function folder(id: number, name: string, role: Folder["role"], accountId = 1): Folder {
  return {
    id,
    account_id: accountId,
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
    color: id === 1 ? "#6da4ff" : "#58c98b",
    icon: "inbox"
  };
}

const folders = [
  folder(1, "Inbox", "inbox"),
  folder(2, "Archive", "archive"),
  folder(3, "Trash", "trash"),
  folder(4, "Drafts", "drafts"),
  folder(5, "All Mail", "all"),
  folder(6, "Projects", "other"),
  folder(9, "Other account", "archive", 2)
];

const tabs = [tab(1, "Primary"), tab(2, "Updates")];

function props(overrides: Record<string, unknown> = {}) {
  return {
    x: 20,
    y: 30,
    message: message(),
    folders,
    tabs,
    onAction: vi.fn(),
    onClose: vi.fn(),
    ...overrides
  };
}

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("MailContextMenu", () => {
  it("shows mail actions without a star action", () => {
    render(MailContextMenu, { props: props() });

    for (const label of [
      "Reply",
      "Reply all",
      "Forward",
      "Mark read",
      "Move to folder",
      "Categorize",
      "Archive",
      "Delete"
    ]) {
      expect(screen.getByRole("menuitem", { name: label })).toBeInTheDocument();
    }
    expect(screen.queryByRole("menuitem", { name: /star/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Mark unread" })).not.toBeInTheDocument();
  });

  it("opens a submenu after the pointer rests on its trigger", async () => {
    vi.useFakeTimers();
    render(MailContextMenu, { props: props() });

    const trigger = screen.getByRole("menuitem", { name: "Move to folder" });
    await fireEvent.pointerEnter(trigger);
    expect(screen.queryByRole("menuitem", { name: "Projects" })).not.toBeInTheDocument();

    vi.advanceTimersByTime(150);
    await tick();

    expect(screen.getByRole("menuitem", { name: "Projects" })).toBeInTheDocument();
  });

  it("shows the category colors in the categorize submenu", async () => {
    const { container } = render(MailContextMenu, { props: props() });

    await fireEvent.click(screen.getByRole("menuitem", { name: "Categorize" }));

    const badges = [...container.querySelectorAll<HTMLElement>(".tab-badge")];
    expect(badges).toHaveLength(2);
    expect(badges.map((badge) => badge.dataset.tabColor)).toEqual([
      "#6da4ff",
      "#58c98b"
    ]);
  });

  it("moves a message through the folder submenu", async () => {
    const onAction = vi.fn();
    const onClose = vi.fn();
    render(MailContextMenu, { props: props({ onAction, onClose }) });

    await fireEvent.click(screen.getByRole("menuitem", { name: "Move to folder" }));

    expect(screen.queryByRole("menuitem", { name: "Inbox" })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Drafts" })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "All Mail" })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Other account" })).not.toBeInTheDocument();

    await fireEvent.click(screen.getByRole("menuitem", { name: "Projects" }));

    expect(onAction).toHaveBeenCalledWith({ type: "move", folderId: 6 });
    expect(onClose).toHaveBeenCalled();
  });

  it("categorizes a message and allows clearing the category", async () => {
    const onAction = vi.fn();
    render(MailContextMenu, { props: props({ onAction }) });

    await fireEvent.click(screen.getByRole("menuitem", { name: "Categorize" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Updates" }));
    expect(onAction).toHaveBeenCalledWith({ type: "categorize", tabId: 2 });

    onAction.mockClear();
    await fireEvent.click(screen.getByRole("menuitem", { name: "Categorize" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Uncategorized" }));
    expect(onAction).toHaveBeenCalledWith({ type: "categorize", tabId: null });
  });

  it("emits reply-all and archive actions", async () => {
    const onAction = vi.fn();
    render(MailContextMenu, { props: props({ onAction }) });

    await fireEvent.click(screen.getByRole("menuitem", { name: "Reply all" }));
    expect(onAction).toHaveBeenCalledWith({ type: "reply-all" });

    onAction.mockClear();
    await fireEvent.click(screen.getByRole("menuitem", { name: "Archive" }));
    expect(onAction).toHaveBeenCalledWith({ type: "archive" });
  });

  it("closes when clicking the backdrop outside the menu", async () => {
    const onClose = vi.fn();
    render(MailContextMenu, { props: props({ onClose }) });

    await fireEvent.click(
      screen.getByRole("button", { name: "Close message menu" })
    );

    expect(onClose).toHaveBeenCalled();
  });

  it("closes on Escape", async () => {
    const onClose = vi.fn();
    render(MailContextMenu, { props: props({ onClose }) });

    await fireEvent.keyDown(window, { key: "Escape" });

    expect(onClose).toHaveBeenCalled();
  });
});
