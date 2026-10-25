import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import type { Space } from "../types";

const mocks = vi.hoisted(() => ({
  listAccounts: vi.fn(),
  listFolders: vi.fn(),
  listDrafts: vi.fn(),
  listTabs: vi.fn(),
  listThreads: vi.fn(),
  tabUnreadCounts: vi.fn(),
  spaceUnreadCounts: vi.fn(),
  setActiveThemeSpace: vi.fn(),
  aiStatus: vi.fn(),
  refreshTrayUnread: vi.fn()
}));

vi.mock("../api", () => ({
  api: {
    listAccounts: mocks.listAccounts,
    listFolders: mocks.listFolders,
    listDrafts: mocks.listDrafts,
    listTabs: mocks.listTabs,
    listThreads: mocks.listThreads,
    tabUnreadCounts: mocks.tabUnreadCounts,
    spaceUnreadCounts: mocks.spaceUnreadCounts,
    setActiveThemeSpace: mocks.setActiveThemeSpace,
    aiStatus: mocks.aiStatus,
    refreshTrayUnread: mocks.refreshTrayUnread
  }
}));

import Sidebar from "./Sidebar.svelte";
import { app } from "../stores/app.svelte";

function space(id: number, name: string): Space {
  return { id, name, icon: "layers", position: id - 1, created_at: 0 };
}

beforeEach(() => {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true
  });
  localStorage.clear();
  app.spaces = [space(1, "Personal"), space(2, "Work")];
  app.activeSpaceId = 1;
  app.accounts = [];
  app.folders = [];
  app.drafts = [];
  app.tabs = [];
  app.messages = [];
  app.selectedFolderId = null;
  app.selectedTabId = null;
  mocks.listAccounts.mockResolvedValue([]);
  mocks.listFolders.mockResolvedValue([]);
  mocks.listDrafts.mockResolvedValue([]);
  mocks.listTabs.mockResolvedValue([]);
  mocks.listThreads.mockResolvedValue([]);
  mocks.tabUnreadCounts.mockResolvedValue({ all: 0, tabs: [] });
  mocks.spaceUnreadCounts.mockResolvedValue([]);
  mocks.setActiveThemeSpace.mockResolvedValue({
    choice: "system",
    accent: null,
    background: "solid",
    panes: "straight"
  });
  mocks.aiStatus.mockResolvedValue(false);
  mocks.refreshTrayUnread.mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("space selector", () => {
  it("shows the space selector above unified folders and switches space", async () => {
    render(Sidebar, {
      props: {
        collapsed: false,
        canToggle: true,
        onToggle: vi.fn(),
        onOpenSettings: vi.fn()
      }
    });

    await fireEvent.click(screen.getByRole("button", { name: /Personal/ }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Work" }));
    expect(app.activeSpaceId).toBe(2);
  });

  it("anchors the space menu outside the scrolling nav", async () => {
    render(Sidebar, {
      props: {
        collapsed: false,
        canToggle: true,
        onToggle: vi.fn(),
        onOpenSettings: vi.fn()
      }
    });

    const trigger = screen.getByRole("button", { name: "Space: Personal" });
    vi.spyOn(trigger, "getBoundingClientRect").mockReturnValue({
      x: 12,
      y: 40,
      left: 12,
      top: 40,
      right: 180,
      bottom: 74,
      width: 168,
      height: 34,
      toJSON: () => ({})
    });

    await fireEvent.click(trigger);

    const menu = screen.getByRole("menu", { name: "Spaces" });
    expect(menu.style.left).toBe("12px");
    expect(menu.style.top).toBe("79px");
  });
});
