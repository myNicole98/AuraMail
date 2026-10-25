import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  Account,
  Folder,
  Message,
  MessageDetail,
  Space,
  Tab,
  ThreadMessage
} from "../types";
import { defaultPreferences, savePreferences } from "../settings";

const mocks = vi.hoisted(() => ({
  getMessage: vi.fn(),
  fillMessageSnippets: vi.fn(),
  listFolders: vi.fn(),
  markRead: vi.fn(),
  toggleStar: vi.fn(),
  trashMessage: vi.fn(),
  deleteMessage: vi.fn(),
  archiveMessage: vi.fn(),
  archiveThread: vi.fn(),
  moveMessageToFolder: vi.fn(),
  setMessageClassification: vi.fn(),
  downloadAttachment: vi.fn(),
  revealDownload: vi.fn(),
  listDrafts: vi.fn(),
  deleteDraft: vi.fn(),
  sendMessage: vi.fn(),
  saveDraft: vi.fn(),
  listAccounts: vi.fn(),
  listSpaces: vi.fn(),
  spaceUnreadCounts: vi.fn(),
  setActiveThemeSpace: vi.fn(),
  listMessages: vi.fn(),
  listThreads: vi.fn(),
  listThreadMessages: vi.fn(),
  syncAccount: vi.fn(),
  listTabs: vi.fn(),
  tabUnreadCounts: vi.fn(),
  createTab: vi.fn(),
  updateTab: vi.fn(),
  deleteTab: vi.fn(),
  reorderTabs: vi.fn(),
  classifyMessage: vi.fn(),
  classifyMessages: vi.fn(),
  classifyUnclassified: vi.fn(),
  getDraft: vi.fn(),
  openComposeWindow: vi.fn(),
  aiStatus: vi.fn(),
  refreshTrayUnread: vi.fn()
}));

vi.mock("../api", () => ({
  api: {
    getMessage: mocks.getMessage,
    fillMessageSnippets: mocks.fillMessageSnippets,
    listFolders: mocks.listFolders,
    markRead: mocks.markRead,
    toggleStar: mocks.toggleStar,
    trashMessage: mocks.trashMessage,
    deleteMessage: mocks.deleteMessage,
    archiveMessage: mocks.archiveMessage,
    archiveThread: mocks.archiveThread,
    moveMessageToFolder: mocks.moveMessageToFolder,
    setMessageClassification: mocks.setMessageClassification,
    downloadAttachment: mocks.downloadAttachment,
    revealDownload: mocks.revealDownload,
    listDrafts: mocks.listDrafts,
    deleteDraft: mocks.deleteDraft,
    sendMessage: mocks.sendMessage,
    saveDraft: mocks.saveDraft,
    listAccounts: mocks.listAccounts,
    listSpaces: mocks.listSpaces,
    spaceUnreadCounts: mocks.spaceUnreadCounts,
    setActiveThemeSpace: mocks.setActiveThemeSpace,
    listMessages: mocks.listMessages,
    listThreads: mocks.listThreads,
    listThreadMessages: mocks.listThreadMessages,
    syncAccount: mocks.syncAccount,
    listTabs: mocks.listTabs,
    tabUnreadCounts: mocks.tabUnreadCounts,
    createTab: mocks.createTab,
    updateTab: mocks.updateTab,
    deleteTab: mocks.deleteTab,
    reorderTabs: mocks.reorderTabs,
    classifyMessage: mocks.classifyMessage,
    classifyMessages: mocks.classifyMessages,
    classifyUnclassified: mocks.classifyUnclassified,
    getDraft: mocks.getDraft,
    openComposeWindow: mocks.openComposeWindow,
    aiStatus: mocks.aiStatus,
    refreshTrayUnread: mocks.refreshTrayUnread
  }
}));

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { app } from "./app.svelte";
import { defaultSpaceAppearance, saveSpaceAppearance, setActiveSpaceId } from "../spaces";

function space(id: number, name: string): Space {
  return { id, name, icon: "layers", position: id - 1, created_at: 0 };
}

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
    created_at: 0,
    ...overrides
  };
}

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
    subject: `Message ${id}`,
    date: 100,
    snippet: "",
    is_seen: false,
    is_starred: false,
    has_attachments: false,
    body_cached: true,
    tab_id: null,
    thread_count: 1,
    thread_unread_count: 1,
    thread_has_inbox: true
  };
}

function detail(id: number): MessageDetail {
  return {
    message: message(id),
    text_body: "body",
    html_body: null,
    image_html: null,
    attachments: []
  };
}

function folder(unread: number): Folder {
  return {
    id: 1,
    account_id: 1,
    path: "INBOX",
    name: "Inbox",
    role: "inbox",
    delimiter: "/",
    uid_validity: 0,
    highest_uid: 0,
    unread_count: unread
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

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((settle) => {
    resolve = settle;
  });
  return { promise, resolve };
}

beforeEach(() => {
  app.destroy();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true
  });
  for (const mock of Object.values(mocks)) mock.mockReset();
  localStorage.clear();
  app.spaces = [space(1, "Personal")];
  app.activeSpaceId = 1;
  app.spaceWorkflows = {};
  setActiveSpaceId(1);
  mocks.listSpaces.mockResolvedValue([space(1, "Personal")]);
  mocks.spaceUnreadCounts.mockResolvedValue([]);
  mocks.setActiveThemeSpace.mockResolvedValue({
    choice: "system",
    accent: null,
    background: "solid",
    panes: "straight"
  });
  mocks.listFolders.mockResolvedValue([]);
  mocks.listThreads.mockResolvedValue([]);
  mocks.listThreadMessages.mockResolvedValue([]);
  mocks.listTabs.mockResolvedValue([]);
  mocks.tabUnreadCounts.mockResolvedValue({ all: 0, tabs: [] });
  mocks.listAccounts.mockResolvedValue([]);
  mocks.listDrafts.mockResolvedValue([]);
  mocks.openComposeWindow.mockResolvedValue(undefined);
  mocks.archiveThread.mockResolvedValue(undefined);
  mocks.moveMessageToFolder.mockResolvedValue(undefined);
  mocks.setMessageClassification.mockResolvedValue({
    tab_id: 2,
    confidence: 1,
    model: "manual"
  });
  mocks.fillMessageSnippets.mockResolvedValue([]);
  mocks.trashMessage.mockResolvedValue(undefined);
  mocks.deleteMessage.mockResolvedValue(undefined);
  app.clearSelection();
  app.messages = [];
  app.folders = [];
  app.drafts = [];
  app.tabs = [];
  app.selectedTabId = null;
  app.selectedFolderId = null;
  app.unified = "inbox";
  app.search = "";
  app.classificationNotice = null;
  app.classificationAction = null;
  app.classifyBusy = false;
  app.classifyingMessageIds = new Set();
  app.flashingMessageIds = new Set();
  app.draftsOpen = false;
  app.closeCompose();
  app.error = null;
  app.statuses = {};
  localStorage.clear();
});

describe("selectMessage", () => {
  it("ignores stale responses from older selections", async () => {
    const first = deferred<MessageDetail>();
    const second = deferred<MessageDetail>();
    mocks.getMessage
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);

    const firstRequest = app.selectMessage(1);
    const secondRequest = app.selectMessage(2);
    expect(app.messageLoading).toBe(true);

    second.resolve(detail(2));
    await secondRequest;
    expect(app.selectedMessage?.message.id).toBe(2);
    expect(app.messageLoading).toBe(false);

    first.resolve(detail(1));
    await firstRequest;
    expect(app.selectedMessage?.message.id).toBe(2);
  });

  it("keeps the current message visible while a cached message loads", async () => {
    app.messages = [message(1), message(2)];
    app.selectedMessageId = 1;
    app.selectedMessage = detail(1);
    const pending = deferred<MessageDetail>();
    mocks.getMessage.mockReturnValue(pending.promise);

    const request = app.selectMessage(2);
    expect(app.selectedMessage?.message.id).toBe(1);
    expect(app.messageLoading).toBe(false);

    pending.resolve(detail(2));
    await request;
    expect(app.selectedMessage?.message.id).toBe(2);
  });

  it("marks an unread message as read after opening it", async () => {
    app.messages = [message(1)];
    app.folders = [folder(1)];
    mocks.getMessage.mockResolvedValue(detail(1));
    mocks.markRead.mockResolvedValue({ ...message(1), is_seen: true });
    mocks.listFolders.mockResolvedValue([folder(0)]);

    await app.selectMessage(1);

    expect(mocks.markRead).toHaveBeenCalledWith(1, true);
    await vi.waitFor(() => expect(app.folders[0].unread_count).toBe(0));
  });

  it("loads the thread messages for the opened message", async () => {
    const first = message(2);
    first.thread_count = 2;
    const second = message(1);
    second.thread_key = first.thread_key;
    app.messages = [first];
    mocks.getMessage.mockResolvedValue({ ...detail(2), message: first });
    mocks.listThreadMessages.mockResolvedValue([first, second]);

    await app.selectMessage(2);

    expect(mocks.listThreadMessages).toHaveBeenCalledWith(1, "thread-2");
    expect(app.threadMessages.map((item) => item.id)).toEqual([2, 1]);
  });
});

describe("delayed mark as read", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    savePreferences({ ...defaultPreferences(), mark_read_delay: 5 });
  });
  afterEach(() => vi.useRealTimers());

  it("waits for the delay before marking read", async () => {
    app.messages = [message(1)];
    mocks.getMessage.mockResolvedValue(detail(1));
    mocks.markRead.mockResolvedValue({ ...message(1), is_seen: true });
    mocks.listFolders.mockResolvedValue([folder(0)]);

    await app.selectMessage(1);
    expect(mocks.markRead).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(4_999);
    expect(mocks.markRead).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(mocks.markRead).toHaveBeenCalledWith(1, true);
  });

  it("cancels the timer when the selection changes", async () => {
    app.messages = [message(1), message(2)];
    mocks.getMessage.mockResolvedValue(detail(1));
    await app.selectMessage(1);
    mocks.getMessage.mockResolvedValue(detail(2));
    await app.selectMessage(2);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(mocks.markRead).not.toHaveBeenCalledWith(1, true);
  });

  it("never marks read when the delay is Never", async () => {
    savePreferences({ ...defaultPreferences(), mark_read_delay: -1 });
    app.messages = [message(1)];
    mocks.getMessage.mockResolvedValue(detail(1));
    await app.selectMessage(1);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(mocks.markRead).not.toHaveBeenCalled();
  });
});

describe("message snippets", () => {
  it("fills empty previews for visible messages", async () => {
    app.messages = [message(1), message(2)];
    app.messages[0].snippet = "";
    app.messages[1].snippet = "Already here";
    mocks.fillMessageSnippets.mockResolvedValue([
      { message_id: 1, snippet: "Fetched body preview" }
    ]);

    await app.fillVisibleSnippets();

    expect(mocks.fillMessageSnippets).toHaveBeenCalledWith([1]);
    expect(app.messages[0].snippet).toBe("Fetched body preview");
  });
});

describe("message actions", () => {
  it("refreshes folder counts after marking read", async () => {
    app.folders = [folder(1)];
    app.selectedMessage = detail(1);
    mocks.markRead.mockResolvedValue({ ...message(1), is_seen: true });
    mocks.listFolders.mockResolvedValue([folder(0)]);

    await app.markRead(true);

    expect(app.folders[0].unread_count).toBe(0);
  });

  it("updates the selected message when starred", async () => {
    app.messages = [message(1)];
    app.selectedMessage = detail(1);
    mocks.toggleStar.mockResolvedValue({ ...message(1), is_starred: true });

    await app.toggleStar();

    expect(app.messages[0].is_starred).toBe(true);
    expect(app.selectedMessage?.message.is_starred).toBe(true);
  });

  it("archives every inbox message in the thread", async () => {
    app.messages = [message(1), message(2)];
    app.messages[1].thread_key = "thread-1";
    mocks.listThreads.mockResolvedValue([]);
    mocks.listFolders.mockResolvedValue([]);

    await app.archiveThread(1, "thread-1");

    expect(mocks.archiveThread).toHaveBeenCalledWith(1, "thread-1");
    expect(app.messages).toEqual([]);
  });

  it("moves a message to another folder and reloads the list", async () => {
    app.messages = [message(1)];
    mocks.listThreads.mockResolvedValue([]);
    mocks.listFolders.mockResolvedValue([]);

    await app.moveMessageToFolder(1, 2);

    expect(mocks.moveMessageToFolder).toHaveBeenCalledWith(1, 2);
    expect(app.messages).toEqual([]);
  });

  it("categorizes a message and refreshes tab counts", async () => {
    app.messages = [message(1)];
    app.tabs = [tab(1, "Primary"), tab(2, "Updates")];
    const classified = { ...message(1), tab_id: 2 };
    mocks.listThreads.mockResolvedValue([classified]);
    mocks.tabUnreadCounts.mockResolvedValue({
      all: 1,
      tabs: [{ tab_id: 2, unread: 1 }]
    });

    await app.categorizeMessage(1, 2);

    expect(mocks.setMessageClassification).toHaveBeenCalledWith(1, 2);
    expect(mocks.listThreads).toHaveBeenCalled();
    expect(app.messages).toEqual([classified]);
    expect(app.tabUnreadCounts.tabs[2]).toBe(1);
    expect(app.flashingMessageIds.has(1)).toBe(true);
  });

  it("shows a manually classified message in the selected tab immediately", async () => {
    app.selectedTabId = 2;
    app.messages = [];
    app.tabs = [tab(1, "Primary"), tab(2, "Updates")];
    const classified = { ...message(1), tab_id: 2 };
    mocks.listThreads.mockResolvedValue([classified]);
    mocks.tabUnreadCounts.mockResolvedValue({
      all: 1,
      tabs: [{ tab_id: 2, unread: 1 }]
    });

    await app.categorizeMessage(1, 2);

    expect(mocks.listThreads).toHaveBeenCalled();
    expect(app.messages).toEqual([classified]);
  });

  it("moves every selected message to a folder", async () => {
    app.messages = [message(1), message(2)];
    mocks.moveMessageToFolder.mockResolvedValue(undefined);
    mocks.listThreads.mockResolvedValue([]);

    await app.moveMessagesToFolder([1, 2], 3);

    expect(mocks.moveMessageToFolder).toHaveBeenCalledTimes(2);
    expect(mocks.moveMessageToFolder).toHaveBeenCalledWith(1, 3);
    expect(mocks.moveMessageToFolder).toHaveBeenCalledWith(2, 3);
    expect(app.messages).toEqual([]);
  });

  it("categorizes every selected message", async () => {
    app.messages = [message(1), message(2)];
    mocks.setMessageClassification.mockResolvedValue({
      tab_id: 2,
      confidence: 1,
      model: "manual"
    });
    mocks.listThreads.mockResolvedValue([]);

    await app.categorizeMessages([1, 2], 2);

    expect(mocks.setMessageClassification).toHaveBeenCalledTimes(2);
    expect(mocks.setMessageClassification).toHaveBeenCalledWith(1, 2);
    expect(mocks.setMessageClassification).toHaveBeenCalledWith(2, 2);
  });

  it("toggles a star from a message row", async () => {
    app.messages = [message(1)];
    mocks.toggleStar.mockResolvedValue({ ...message(1), is_starred: true });

    await app.toggleStarMessage(1);

    expect(mocks.toggleStar).toHaveBeenCalledWith(1);
    expect(app.messages[0].is_starred).toBe(true);
  });

  it("opens the composer for a row action", async () => {
    mocks.getMessage.mockResolvedValue(detail(1));

    await app.composeMessage(1, "reply-all");

    expect(mocks.openComposeWindow).toHaveBeenCalledWith("reply-all", 1, null);
  });

  it("queues a thread archive while syncing", async () => {
    app.messages = [message(1), message(2)];
    app.messages[1].thread_key = "thread-1";
    app.statuses = {
      1: { accountId: 1, state: "syncing", message: undefined }
    };
    mocks.listThreads.mockResolvedValue([]);
    mocks.listFolders.mockResolvedValue([]);

    await app.archiveThread(1, "thread-1");

    expect(mocks.archiveThread).not.toHaveBeenCalled();
    expect(app.messages).toEqual([]);

    app.statuses = {};
    await app.flushQueuedActions();

    expect(mocks.archiveThread).toHaveBeenCalledWith(1, "thread-1");
    expect(app.messages).toEqual([]);
  });
});

describe("search", () => {
  it("searches every folder and account without the active scope", async () => {
    app.selectedFolderId = 1;
    app.selectedTabId = 2;
    app.search = "needle";
    mocks.listThreads.mockResolvedValue([]);

    await app.refresh();

    expect(app.searchActive).toBe(true);
    expect(mocks.listThreads).toHaveBeenCalledWith({
      space_id: 1,
      search: "needle",
      limit: 200,
      order: "newest"
    });
  });

  it("returns to the active folder view when the search is cleared", async () => {
    app.selectedFolderId = 1;
    app.search = "";
    mocks.listThreads.mockResolvedValue([]);

    await app.refresh();

    expect(app.searchActive).toBe(false);
    expect(mocks.listThreads).toHaveBeenCalledWith({
      space_id: 1,
      folder_id: 1,
      tab_id: undefined,
      unified: undefined,
      limit: 200,
      order: "newest"
    });
  });
});

describe("message scope switching", () => {
  it("loads the message list without refreshing unrelated app metadata", async () => {
    mocks.listThreads.mockResolvedValue([message(7)]);

    await app.selectUnified("archive");

    expect(mocks.listThreads).toHaveBeenCalledWith({
      space_id: 1,
      folder_id: undefined,
      tab_id: undefined,
      unified: "archive",
      limit: 200,
      order: "newest"
    });
    expect(mocks.listAccounts).not.toHaveBeenCalled();
    expect(mocks.listFolders).not.toHaveBeenCalled();
    expect(mocks.listDrafts).not.toHaveBeenCalled();
    expect(mocks.listTabs).not.toHaveBeenCalled();
    expect(app.messages.map((entry) => entry.id)).toEqual([7]);
  });

  it("does not let an older folder response overwrite a newer scope", async () => {
    const archive = deferred<ThreadMessage[]>();
    const spam = deferred<ThreadMessage[]>();
    mocks.listThreads
      .mockReturnValueOnce(archive.promise)
      .mockReturnValueOnce(spam.promise);

    const archiveRequest = app.selectUnified("archive");
    const spamRequest = app.selectUnified("spam");

    spam.resolve([message(2)]);
    await spamRequest;
    expect(app.messages.map((entry) => entry.id)).toEqual([2]);

    archive.resolve([message(1)]);
    await archiveRequest;
    expect(app.messages.map((entry) => entry.id)).toEqual([2]);
  });
});

describe("conversation order", () => {
  it("requests the stored conversation order", async () => {
    savePreferences({ ...defaultPreferences(), message_order: "oldest" });
    mocks.listAccounts.mockResolvedValue([]);
    mocks.listFolders.mockResolvedValue([]);
    mocks.listDrafts.mockResolvedValue([]);
    mocks.listTabs.mockResolvedValue([]);
    mocks.tabUnreadCounts.mockResolvedValue({ tabs: [] });
    mocks.listThreads.mockResolvedValue([]);

    await app.refresh();

    expect(mocks.listThreads).toHaveBeenCalledWith(
      expect.objectContaining({ order: "oldest" })
    );
  });
});

describe("classification", () => {
  it("classifies the selected message and reloads", async () => {
    app.selectedMessage = detail(1);
    app.tabs = [tab(1, "Primary")];
    mocks.classifyMessage.mockResolvedValue({
      tab_id: 1,
      confidence: 1,
      model: "test"
    });
    mocks.listThreads.mockResolvedValue([]);

    await app.classifySelected();

    expect(mocks.classifyMessage).toHaveBeenCalledWith(1, true);
    expect(app.flashingMessageIds.has(1)).toBe(true);
  });

  it("marks a message while it is being classified", async () => {
    const pending = deferred<{ tab_id: number; confidence: number; model: string }>();
    mocks.classifyMessage.mockReturnValue(pending.promise);
    mocks.listThreads.mockResolvedValue([]);

    const run = app.classifyMessageById(1);
    expect(app.classifyingMessageIds.has(1)).toBe(true);

    pending.resolve({ tab_id: 1, confidence: 1, model: "test" });
    await run;
    expect(app.classifyingMessageIds.has(1)).toBe(false);
  });

  it("applies auto classifications without a folder switch", async () => {
    app.messages = [message(1)];
    app.tabs = [tab(1, "Primary"), tab(2, "Updates")];
    mocks.tabUnreadCounts.mockResolvedValue({
      all: 1,
      tabs: [{ tab_id: 2, unread: 1 }]
    });

    app.handleClassificationStarted([1]);
    expect(app.classifyingMessageIds.has(1)).toBe(true);

    await app.handleClassificationDone({
      accountId: 1,
      classified: 1,
      failed: 0,
      results: [
        {
          message_id: 1,
          classification: { tab_id: 2, confidence: 0.9, model: "test" },
          error: null
        }
      ]
    });

    expect(app.classifyingMessageIds.has(1)).toBe(false);
    expect(app.messages[0].tab_id).toBe(2);
    expect(app.flashingMessageIds.has(1)).toBe(true);
    expect(app.tabUnreadCounts.tabs[2]).toBe(1);
    expect(app.classificationNotice).toContain("Classified 1 message");
  });

  it("does not flash a failed auto classification", async () => {
    app.messages = [message(1)];
    mocks.tabUnreadCounts.mockResolvedValue({ all: 1, tabs: [] });

    app.handleClassificationStarted([1]);
    await app.handleClassificationDone({
      accountId: 1,
      classified: 0,
      failed: 1,
      results: [{ message_id: 1, classification: null, error: "nope" }]
    });

    expect(app.classifyingMessageIds.has(1)).toBe(false);
    expect(app.flashingMessageIds.has(1)).toBe(false);
    expect(app.messages[0].tab_id).toBeNull();
  });

  it("summarizes visible classification outcomes", async () => {
    app.messages = [message(1), message(2), message(3)];
    mocks.classifyMessage.mockImplementation((id: number) => {
      if (id === 3) return Promise.reject(new Error("failed"));
      return Promise.resolve({
        tab_id: id === 1 ? 1 : null,
        confidence: 1,
        model: "x"
      });
    });
    mocks.listThreads.mockResolvedValue([]);

    await app.classifyVisible();

    expect(mocks.listThreads).toHaveBeenCalledTimes(3);
    expect(app.flashingMessageIds.has(1)).toBe(true);
    expect(app.flashingMessageIds.has(2)).toBe(false);
    expect(app.flashingMessageIds.has(3)).toBe(false);
    expect(app.classificationNotice).toBe("Classified 2 messages.");
  });

  it("shows progress and a completion notice for multiple messages", async () => {
    const pending = deferred<{ tab_id: number; confidence: number; model: string }>();
    app.messages = [message(1), message(2)];
    mocks.classifyMessage.mockReturnValueOnce(pending.promise).mockResolvedValue({
      tab_id: 1,
      confidence: 1,
      model: "test"
    });
    mocks.listThreads.mockResolvedValue([]);

    const run = app.classifyVisible();
    expect(app.classificationProgress).toEqual({
      total: 2,
      done: 0,
      aborting: false,
      abortable: true
    });

    pending.resolve({ tab_id: 1, confidence: 1, model: "test" });
    await run;

    expect(app.classificationProgress).toBeNull();
    expect(app.classificationNotice).toBe("Classified 2 messages.");
  });

  it("aborts after the current message and reports the partial count", async () => {
    const pending = deferred<{ tab_id: number; confidence: number; model: string }>();
    app.messages = [message(1), message(2), message(3)];
    mocks.classifyMessage.mockReturnValue(pending.promise);
    mocks.listThreads.mockResolvedValue([]);

    const run = app.classifyVisible();
    expect(app.classificationProgress?.done).toBe(0);
    app.abortClassification();
    expect(app.classificationProgress?.aborting).toBe(true);

    pending.resolve({ tab_id: 1, confidence: 1, model: "test" });
    await run;

    expect(mocks.classifyMessage).toHaveBeenCalledTimes(1);
    expect(app.classificationProgress).toBeNull();
    expect(app.classificationNotice).toBe(
      "Classified 1 of 3 messages before stopping."
    );
  });

  it("shows a completion notice for a manually classified message", async () => {
    app.selectedMessage = detail(1);
    mocks.classifyMessage.mockResolvedValue({
      tab_id: 1,
      confidence: 1,
      model: "test"
    });
    mocks.listThreads.mockResolvedValue([]);

    await app.classifySelected();

    expect(app.classificationProgress).toBeNull();
    expect(app.classificationNotice).toBe("Classified 1 message.");
  });
});

describe("tabs", () => {
  it("loads unread-thread counts for the current section", async () => {
    app.selectedFolderId = 1;
    mocks.tabUnreadCounts.mockResolvedValue({
      all: 5,
      tabs: [{ tab_id: 2, unread: 3 }]
    });
    mocks.listThreads.mockResolvedValue([]);

    await app.refresh();

    expect(mocks.tabUnreadCounts).toHaveBeenCalledWith({
      space_id: 1,
      folder_id: 1
    });
    expect(app.tabUnreadCounts.all).toBe(5);
    expect(app.tabUnreadCounts.tabs[2]).toBe(3);
    expect(app.unreadCountFor(2)).toBe(3);
    expect(app.unreadCountFor(99)).toBe(0);
  });

  it("does not ask for reclassification when a tab is created", async () => {
    const tab = {
      id: 1,
      space_id: 1,
      name: "People",
      description: "People I know.",
      position: 0,
      removable: true,
      primary: true,
      color: "#6da4ff",
      icon: "inbox"
    };
    mocks.createTab.mockResolvedValue(tab);
    mocks.listTabs.mockResolvedValue([tab]);
    mocks.listThreads.mockResolvedValue([]);

    const error = await app.createTab(tab.name, tab.description, tab.color, tab.icon);

    expect(error).toBeNull();
    expect(app.classificationAction).toBeNull();
    expect(app.classificationNotice).toBeNull();
  });

  it("does not ask for reclassification when a tab is deleted", async () => {
    const tab = {
      id: 1,
      space_id: 1,
      name: "People",
      description: "People I know.",
      position: 0,
      removable: true,
      primary: true,
      color: "#6da4ff",
      icon: "inbox"
    };
    app.tabs = [tab];
    app.selectedTabId = tab.id;
    mocks.deleteTab.mockResolvedValue(undefined);
    mocks.listTabs.mockResolvedValue([]);
    mocks.listThreads.mockResolvedValue([]);

    const error = await app.deleteTab(tab.id);

    expect(error).toBeNull();
    expect(app.classificationAction).toBeNull();
    expect(app.classificationNotice).toBeNull();
  });

  it("does not ask for reclassification when only the name changes", async () => {
    const tab = {
      id: 1,
      space_id: 1,
      name: "Primary",
      description: "Direct mail.",
      position: 0,
      removable: true,
      primary: true,
      color: "#6da4ff",
      icon: "inbox"
    };
    app.tabs = [tab];
    mocks.updateTab.mockResolvedValue({ ...tab, name: "People" });
    mocks.listTabs.mockResolvedValue([{ ...tab, name: "People" }]);
    mocks.listThreads.mockResolvedValue([]);

    const error = await app.updateTab(1, "People", tab.description, tab.color, tab.icon);

    expect(error).toBeNull();
    expect(mocks.updateTab).toHaveBeenCalledWith(
      1,
      "People",
      "Direct mail.",
      "#6da4ff",
      "inbox",
      1
    );
    expect(app.classificationAction).toBeNull();
  });

  it("does not ask for reclassification when the description changes", async () => {
    const tab = {
      id: 1,
      space_id: 1,
      name: "Primary",
      description: "Direct mail.",
      position: 0,
      removable: true,
      primary: true,
      color: "#6da4ff",
      icon: "inbox"
    };
    app.tabs = [tab];
    mocks.updateTab.mockResolvedValue({ ...tab, description: "People I know." });
    mocks.listTabs.mockResolvedValue([{ ...tab, description: "People I know." }]);
    mocks.listThreads.mockResolvedValue([]);

    await app.updateTab(1, tab.name, "People I know.", tab.color, tab.icon);

    expect(app.classificationAction).toBeNull();
    expect(app.classificationNotice).toBeNull();
  });

  it("reorders tabs and keeps the backend order", async () => {
    const first = {
      id: 1, space_id: 1, name: "One", description: "One.", position: 0, removable: true,
      primary: true, color: "#6da4ff", icon: "inbox"
    };
    const second = {
      id: 2, space_id: 1, name: "Two", description: "Two.", position: 1, removable: true,
      primary: false, color: "#58c98b", icon: "bell"
    };
    app.tabs = [first, second];
    mocks.reorderTabs.mockResolvedValue([second, first]);

    const error = await app.reorderTabs([2, 1]);

    expect(error).toBeNull();
    expect(mocks.reorderTabs).toHaveBeenCalledWith([2, 1], 1);
    expect(app.tabs.map((tab) => tab.id)).toEqual([2, 1]);
  });
});

describe("compose and drafts", () => {
  it("opens compose with a mode and closes it", () => {
    app.openCompose("reply", detail(3));
    expect(app.composerOpen).toBe(true);
    expect(app.composerMode).toBe("reply");
    expect(app.composerSource?.message.id).toBe(3);
    expect(mocks.openComposeWindow).toHaveBeenCalledWith("reply", 3, null);

    app.closeCompose();
    expect(app.composerOpen).toBe(false);
    expect(app.composerSource).toBeNull();
  });

  it("loads drafts and a full draft payload", async () => {
    const summary = {
      id: 4,
      accountId: 1,
      toAddrs: ["ada@example.com"],
      ccAddrs: [],
      subject: "Draft",
      snippet: "Body",
      hasAttachments: false,
      updatedAt: 100
    };
    const draft = {
      id: 4,
      accountId: 1,
      toAddrs: ["ada@example.com"],
      ccAddrs: [],
      subject: "Draft",
      body: "Body",
      bodyHtml: null,
      inlineImages: [],
      inReplyTo: null,
      references: null,
      attachments: [],
      updatedAt: 100
    };
    mocks.listDrafts.mockResolvedValue([summary]);
    mocks.getDraft.mockResolvedValue(draft);

    await app.loadDrafts();
    await app.openDraft(4);

    expect(app.drafts).toEqual([summary]);
    expect(app.composerDraft).toEqual(draft);
  });

  it("reports when the compose window cannot open", async () => {
    const draft = {
      id: 4,
      accountId: 1,
      toAddrs: ["ada@example.com"],
      ccAddrs: [],
      subject: "Draft",
      body: "Body",
      bodyHtml: null,
      inlineImages: [],
      inReplyTo: null,
      references: null,
      attachments: [],
      updatedAt: 100
    };
    mocks.getDraft.mockResolvedValue(draft);
    mocks.openComposeWindow.mockRejectedValueOnce(new Error("window failed"));

    expect(await app.openDraft(4)).toBe(false);
    expect(app.error).toBe("window failed");
  });

  it("switching spaces restores each workflow", async () => {
    app.spaces = [space(1, "Personal"), space(2, "Work")];
    app.activeSpaceId = 1;
    app.selectedFolderId = 12;
    mocks.listFolders.mockResolvedValueOnce([]);
    await app.switchSpace(2);
    expect(app.activeSpaceId).toBe(2);
    expect(app.selectedFolderId).toBeNull();
    app.selectedFolderId = 22;
    mocks.listFolders.mockResolvedValueOnce([{ ...folder(0), id: 12 }]);
    await app.switchSpace(1);
    expect(app.activeSpaceId).toBe(1);
    expect(app.selectedFolderId).toBe(12);
  });

  it("applies each space appearance immediately when switching", async () => {
    app.spaces = [space(1, "Personal"), space(2, "Work")];
    app.activeSpaceId = 1;
    saveSpaceAppearance(1, {
      ...defaultSpaceAppearance(),
      theme_choice: "dark",
      theme_background: "solid",
      theme_panes: "straight"
    });
    saveSpaceAppearance(2, {
      ...defaultSpaceAppearance(),
      theme_choice: "light",
      theme_background: "glass",
      theme_panes: "rounded"
    });

    await app.switchSpace(2);
    expect(app.spaceSwitching).toBe(false);
    expect(document.documentElement.dataset.themeBackground).toBe("glass");
    expect(document.documentElement.dataset.themePanes).toBe("rounded");

    await app.switchSpace(1);
    expect(document.documentElement.dataset.themeBackground).toBe("solid");
    expect(document.documentElement.dataset.themePanes).toBe("straight");
  });

  it("deletes a draft and reports failure", async () => {
    mocks.deleteDraft.mockResolvedValue(undefined);
    mocks.listDrafts.mockResolvedValue([]);

    expect(await app.discardDraft(4)).toBe(true);
    expect(app.drafts).toEqual([]);

    mocks.deleteDraft.mockRejectedValue("delete failed");
    expect(await app.discardDraft(4)).toBe(false);
    expect(app.error).toBe("delete failed");
  });
});

describe("attachment reveal", () => {
  beforeEach(() => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true
    });
  });
  afterEach(() => {
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  });

  it("reveals a saved file when the preference is on", async () => {
    savePreferences({ ...defaultPreferences(), reveal_download: true });
    mocks.downloadAttachment.mockResolvedValue("/tmp/report.pdf");
    mocks.revealDownload.mockResolvedValue(undefined);

    await app.downloadAttachment(7);

    expect(mocks.revealDownload).toHaveBeenCalledWith("/tmp/report.pdf");
    expect(app.attachmentNotice).toBe("Saved to /tmp/report.pdf");
  });

  it("keeps the save notice when revealing fails", async () => {
    savePreferences({ ...defaultPreferences(), reveal_download: true });
    mocks.downloadAttachment.mockResolvedValue("/tmp/report.pdf");
    mocks.revealDownload.mockRejectedValue(new Error("no file manager"));

    await app.downloadAttachment(7);

    expect(app.attachmentNotice).toBe("Saved to /tmp/report.pdf");
    expect(app.error).toBeNull();
  });

  it("does not reveal when the preference is off", async () => {
    savePreferences({ ...defaultPreferences(), reveal_download: false });
    mocks.downloadAttachment.mockResolvedValue("/tmp/report.pdf");

    await app.downloadAttachment(7);

    expect(mocks.revealDownload).not.toHaveBeenCalled();
    expect(app.attachmentNotice).toBe("Saved to /tmp/report.pdf");
  });

  it("does not reveal when the save fails", async () => {
    savePreferences({ ...defaultPreferences(), reveal_download: true });
    mocks.downloadAttachment.mockRejectedValue(new Error("disk full"));

    await app.downloadAttachment(7);

    expect(mocks.revealDownload).not.toHaveBeenCalled();
    expect(app.attachmentNotice).toBeNull();
    expect(app.error).toBe("disk full");
  });
});

describe("AI reply composer queue", () => {
  it("queues a draft for the active composer generation", () => {
    app.openCompose("reply", detail(3));
    const token = app.startComposerAiDraft();
    expect(app.composerAiDrafting).toBe(true);

    expect(app.applyComposerAiDraft(token, "Draft text")).toBe(true);
    expect(app.composerAiDraft).toEqual({ token, text: "Draft text" });
    expect(app.composerAiDrafting).toBe(false);
  });

  it("discards drafts and errors from stale composer generations", () => {
    app.openCompose("reply", detail(3));
    const token = app.startComposerAiDraft();
    app.openCompose("reply", detail(4));

    expect(app.applyComposerAiDraft(token, "stale draft")).toBe(false);
    expect(app.failComposerAiDraft(token, "stale error")).toBe(false);
    expect(app.composerAiDraft).toBeNull();
    expect(app.composerAiError).toBeNull();
  });

  it("invalidates a pending reply when the composer closes", () => {
    app.openCompose("reply", detail(3));
    const token = app.startComposerAiDraft();
    app.closeCompose();

    expect(app.applyComposerAiDraft(token, "late draft")).toBe(false);
    expect(app.composerAiDrafting).toBe(false);
  });
});

describe("sync state", () => {
  it("reports a reconnect state for expired Google authorization", () => {
    app.accounts = [account({ id: 3, email: "me@gmail.com" })];
    app.statuses = {
      3: { accountId: 3, state: "reauth", message: "Reconnect this account." }
    };
    expect(app.syncState).toBe("reauth");
    expect(app.syncLabel).toBe("Reconnect needed");
    expect(app.reauthAccount?.email).toBe("me@gmail.com");
  });
});
