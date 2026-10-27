import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "../api";
import { applyMatugenPalette, loadPreferences } from "../settings";
import {
  activeSpaceId,
  loadSpaceAppearance,
  loadSpaceWorkflow,
  migrateLegacyAppearance,
  migrateLegacyGeneral,
  saveSpaceWorkflow,
  setActiveSpaceId
} from "../spaces";
import { appearance } from "./appearance.svelte";
import type {
  Account,
  Classification,
  ClassificationDone,
  ClassificationOutcome,
  ClassificationProgress,
  ComposeMode,
  Draft,
  DraftSummary,
  Folder,
  Message,
  MessageDetail,
  MessageQuery,
  ProtonAccessLimitedEvent,
  Space,
  SpaceWorkflow,
  ThreadMessage,
  SyncFolderStatus,
  SyncState,
  SyncStatus,
  Tab
} from "../types";

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Something went wrong";
}



class AppStore {
  spaces = $state<Space[]>([]);
  activeSpaceId = $state<number | null>(null);
  spaceSwitching = $state(false);
  spaceUnreadCounts = $state<Record<number, number>>({});
  spaceAttention = $state<Record<number, boolean>>({});
  spaceWorkflows = $state<Record<number, SpaceWorkflow>>({});
  accounts = $state<Account[]>([]);
  folders = $state<Folder[]>([]);
  tabs = $state<Tab[]>([]);
  tabUnreadCounts = $state<{ all: number; tabs: Record<number, number> }>({
    all: 0,
    tabs: {}
  });
  messages = $state<ThreadMessage[]>([]);
  drafts = $state<DraftSummary[]>([]);
  draftsOpen = $state(false);
  selectedFolderId = $state<number | null>(null);
  selectedTabId = $state<number | null>(null);
  unified = $state<"inbox" | "sent" | "archive" | "spam" | "trash" | "starred">("inbox");
  classifyBusy = $state(false);
  classifyingMessageIds = $state<Set<number>>(new Set());
  flashingMessageIds = $state<Set<number>>(new Set());
  classificationNotice = $state<string | null>(null);
  classificationAction = $state<"reclassify" | null>(null);
  classificationProgress = $state<ClassificationProgress | null>(null);
  private classificationAbort = false;
  selectedMessageId = $state<number | null>(null);
  selectedMessage = $state<MessageDetail | null>(null);
  threadMessages = $state<Message[]>([]);
  composerOpen = $state(false);
  composerMode = $state<ComposeMode>("new");
  composerSource = $state<MessageDetail | null>(null);
  composerDraft = $state<Draft | null>(null);
  composerAiDraft = $state<{ token: number; text: string } | null>(null);
  composerAiError = $state<{ token: number; message: string } | null>(null);
  composerAiDrafting = $state(false);
  aiConfigured = $state(false);
  auraRailOpen = $state(false);
  search = $state("");
  statuses = $state<Record<number, SyncStatus>>({});
  protonAccessLimited = $state<ProtonAccessLimitedEvent | null>(null);
  syncingFolders = $state<Record<number, boolean>>({});
  loading = $state(false);
  messageLoading = $state(false);
  messageLimit = $state(200);
  loadingMore = $state(false);
  noMoreMessages = $state(false);
  attachmentNotice = $state<string | null>(null);
  themeNotice = $state<string | null>(null);
  error = $state<string | null>(null);
  booted = $state(false);
  private initialized = false;
  private composerGeneration = 0;
  private selectionRequest = 0;
  private messageRequest = 0;
  private renderRequest = 0;
  private tabRequest = 0;
  private messageCache = new Map<string, ThreadMessage[]>();
  private prefetchTimer: ReturnType<typeof setTimeout> | undefined;
  private messageScopeLoading = false;
  private progressiveRenderActive = false;
  private searchTimer: ReturnType<typeof setTimeout> | undefined;
  private snippetAttempts = new Set<number>();
  private noticeTimer: ReturnType<typeof setTimeout> | undefined;
  private themeTimer: ReturnType<typeof setTimeout> | undefined;
  private markReadTimer: ReturnType<typeof setTimeout> | undefined;
  private unlisteners: UnlistenFn[] = [];
  private hiddenMessageIds = new Set<number>();
  private queuedActions: { ids: number[]; run: () => Promise<void> }[] = [];
  private flushingActions = false;
  private flashTimers = new Map<number, ReturnType<typeof setTimeout>>();

  get activeSpace(): Space | null {
    return (
      this.spaces.find((space) => space.id === this.activeSpaceId) ?? null
    );
  }

  get activeAccounts(): Account[] {
    if (this.activeSpaceId === null) return [];
    return this.accounts.filter(
      (account) => account.space_id === this.activeSpaceId
    );
  }

  get activeFolders(): Folder[] {
    const ids = new Set(this.activeAccounts.map((account) => account.id));
    return this.folders.filter((folder) => ids.has(folder.account_id));
  }

  get syncing(): boolean {
    return Object.values(this.statuses).some(
      (status) => status.state === "syncing"
    );
  }

  get syncState(): SyncState | "online" {
    const states = this.activeAccounts.map(
      (account) => this.statuses[account.id]?.state ?? "idle"
    );
    if (states.includes("syncing")) return "syncing";
    if (states.includes("offline")) return "offline";
    if (states.includes("reauth")) return "reauth";
    if (states.includes("error")) return "error";
    return "online";
  }

  get reauthAccount(): Account | null {
    const entry = Object.entries(this.statuses)
      .find(([, status]) => status.state === "reauth");
    if (!entry) return null;
    return this.accounts.find((account) => account.id === Number(entry[0])) ?? null;
  }

  clearProtonAccessLimited(username?: string): void {
    if (!this.protonAccessLimited) return;
    if (username && this.protonAccessLimited.username !== username) return;
    this.protonAccessLimited = null;
  }

  get syncLabel(): string {
    const accounts = this.activeAccounts;
    if (accounts.length === 0) return "No accounts";
    if (this.syncing) return "Syncing…";
    if (this.syncState === "offline") return "Offline · retrying…";
    if (this.syncState === "reauth") return "Reconnect needed";
    if (this.syncState === "error") return "Sync failed · retrying…";
    const count = accounts.length;
    return `Online · ${count} ${count === 1 ? "account" : "accounts"}`;
  }

  openAccountSettings(accountId: number): void {
    window.dispatchEvent(new CustomEvent("auramail:open-settings", {
      detail: { tab: "account", accountId }
    }));
  }

  get searchActive(): boolean {
    return this.search.trim().length > 0;
  }

  unreadCountFor(tabId: number): number {
    return this.tabUnreadCounts.tabs[tabId] ?? 0;
  }

  private tabCountScope(): Pick<
    MessageQuery,
    "folder_id" | "account_id" | "space_id" | "unified"
  > {
    return {
      folder_id: this.selectedFolderId ?? undefined,
      space_id: this.activeSpaceId ?? undefined,
      unified: this.selectedFolderId === null ? this.unified : undefined
    };
  }

  private async fetchTabUnreadCounts(): Promise<{
    all: number;
    tabs: Record<number, number>;
  }> {
    try {
      const counts = await api.tabUnreadCounts(this.tabCountScope());
      return {
        all: counts.all,
        tabs: Object.fromEntries(
          counts.tabs.map((tab) => [tab.tab_id, tab.unread])
        )
      };
    } catch {
      return this.tabUnreadCounts;
    }
  }

  private async refreshTabCounts(): Promise<void> {
    const request = ++this.tabRequest;
    const counts = await this.fetchTabUnreadCounts();
    if (request === this.tabRequest) this.tabUnreadCounts = counts;
  }

  async init(): Promise<void> {
    if (this.initialized) return;
    this.initialized = true;
    if (!isTauri()) {
      this.booted = true;
      return;
    }
    try {
      await this.bindEvents();
      await this.loadSpaces();
      await this.refresh();
      void this.refreshAiStatus();
      if (this.accounts.length > 0) void this.syncAll();
    } finally {
      this.booted = true;
    }
  }

  private async loadSpaces(): Promise<void> {
    if (!isTauri()) return;
    try {
      this.spaces = await api.listSpaces();
      if (this.spaces.length === 0) {
        this.activeSpaceId = null;
        return;
      }
      const defaultSpace =
        this.spaces.find((space) => space.name.toLowerCase() === "default") ??
        this.spaces[0];
      migrateLegacyAppearance(defaultSpace.id);
      migrateLegacyGeneral(
        defaultSpace.id,
        this.spaces.map((space) => space.id)
      );
      const saved = activeSpaceId();
      const active =
        this.spaces.find((space) => space.id === saved) ?? this.spaces[0];
      this.activeSpaceId = active.id;
      setActiveSpaceId(active.id);
    } catch (error) {
      this.error = errorMessage(error);
    }
  }

  async refreshSpaceUnreadCounts(): Promise<void> {
    if (!isTauri()) return;
    try {
      const counts = await api.spaceUnreadCounts();
      this.spaceUnreadCounts = Object.fromEntries(
        counts.map((count) => [count.space_id, count.unread])
      );
    } catch {
      return;
    }
  }

  private noteSpaceActivity(accountId: number): void {
    const account = this.accounts.find((candidate) => candidate.id === accountId);
    if (!account || account.space_id === this.activeSpaceId) return;
    this.spaceAttention = {
      ...this.spaceAttention,
      [account.space_id]: true
    };
  }

  async refreshAiStatus(): Promise<void> {
    if (!isTauri()) return;
    try {
      this.aiConfigured = await api.aiStatus(this.activeSpaceId ?? undefined);
    } catch {
      this.aiConfigured = false;
    }
  }

  openAuraRail(): void {
    if (this.selectedMessageId !== null) this.auraRailOpen = true;
  }

  closeAuraRail(): void {
    this.auraRailOpen = false;
  }

  toggleAuraRail(): void {
    if (this.selectedMessageId === null) return;
    this.auraRailOpen = !this.auraRailOpen;
  }

  startComposerAiDraft(): number {
    this.composerGeneration += 1;
    this.composerAiDraft = null;
    this.composerAiError = null;
    this.composerAiDrafting = true;
    return this.composerGeneration;
  }

  isComposerGeneration(token: number): boolean {
    return token === this.composerGeneration;
  }

  applyComposerAiDraft(token: number, text: string): boolean {
    if (!this.isComposerGeneration(token) || !this.composerOpen) return false;
    this.composerAiDraft = { token, text };
    this.composerAiDrafting = false;
    return true;
  }

  failComposerAiDraft(token: number, message: string): boolean {
    if (!this.isComposerGeneration(token) || !this.composerOpen) return false;
    this.composerAiError = { token, message };
    this.composerAiDrafting = false;
    return true;
  }

  async refresh(): Promise<void> {
    if (!isTauri()) return;
    if (this.spaces.length === 0) await this.loadSpaces();
    if (this.activeSpaceId === null) {
      this.accounts = [];
      this.folders = [];
      this.drafts = [];
      this.tabs = [];
      this.messages = [];
      this.protonAccessLimited = null;
      return;
    }
    this.loading = true;
    try {
      const counts = this.fetchTabUnreadCounts();
      const spaceId = this.activeSpaceId;
      const [accounts, folders, drafts, tabs] = await Promise.all([
        api.listAccounts(),
        api.listFolders(undefined, spaceId),
        api.listDrafts(undefined, spaceId),
        api.listTabs(spaceId)
      ]);
      this.accounts = accounts;
      const limited = this.protonAccessLimited;
      if (
        limited &&
        !accounts.some((account) => account.username === limited.username)
      ) {
        this.protonAccessLimited = null;
      }
      this.folders = folders;
      this.drafts = drafts;
      this.tabs = tabs;
      this.tabUnreadCounts = await counts;
      await this.refreshSpaceUnreadCounts();
      this.refreshTrayUnread();
      if (
        this.selectedTabId !== null &&
        !tabs.some((tab) => tab.id === this.selectedTabId)
      ) {
        this.selectedTabId = null;
      }
      const query = this.messageQuery();
      const key = JSON.stringify(query);
      this.invalidateMessageCache();
      const threads = await api.listThreads(query);
      const request = ++this.messageRequest;
      this.applyMessages(this.filterHidden(threads), request, false);
      this.rememberMessages(key, threads);
      this.scheduleUnifiedPrefetch();
      this.error = null;
    } catch (error) {
      this.error = errorMessage(error);
    } finally {
      this.loading = false;
    }
  }

  private resetPagination(): void {
    this.messageLimit = 200;
    this.noMoreMessages = false;
  }

  private messageQuery(): MessageQuery {
    const search = this.search.trim();
    const order = loadPreferences().message_order;
    if (search) {
      return {
        space_id: this.activeSpaceId ?? undefined,
        search,
        limit: this.messageLimit,
        order
      };
    }
    return {
      space_id: this.activeSpaceId ?? undefined,
      folder_id: this.selectedFolderId ?? undefined,
      tab_id: this.selectedTabId ?? undefined,
      unified: this.selectedFolderId === null ? this.unified : undefined,
      limit: this.messageLimit,
      order
    };
  }

  async loadMoreMessages(): Promise<void> {
    if (
      !isTauri() ||
      this.loadingMore ||
      this.noMoreMessages ||
      this.draftsOpen ||
      this.messageScopeLoading ||
      this.progressiveRenderActive
    ) {
      return;
    }
    if (this.messages.length < this.messageLimit) {
      this.noMoreMessages = true;
      return;
    }
    this.loadingMore = true;
    try {
      this.messageLimit = Math.min(this.messageLimit + 100, 5000);
      await this.reloadMessages();
      if (this.messages.length < this.messageLimit) this.noMoreMessages = true;
      this.error = null;
    } catch (error) {
      this.error = errorMessage(error);
    } finally {
      this.loadingMore = false;
    }
  }

  private saveCurrentWorkflow(): void {
    if (this.activeSpaceId === null) return;
    const workflow: SpaceWorkflow = {
      selectedFolderId: this.selectedFolderId,
      selectedTabId: this.selectedTabId,
      selectedMessageId: this.selectedMessageId,
      unified: this.unified,
      draftsOpen: this.draftsOpen
    };
    this.spaceWorkflows = {
      ...this.spaceWorkflows,
      [this.activeSpaceId]: workflow
    };
    saveSpaceWorkflow(this.activeSpaceId, workflow);
  }

  private restoreWorkflow(workflow: SpaceWorkflow): void {
    this.unified = workflow.unified;
    this.draftsOpen =
      workflow.draftsOpen &&
      workflow.selectedFolderId === null &&
      workflow.selectedTabId === null;
    this.selectedFolderId =
      workflow.selectedFolderId !== null &&
      this.folders.some((folder) => folder.id === workflow.selectedFolderId)
        ? workflow.selectedFolderId
        : null;
    this.selectedTabId =
      workflow.selectedTabId !== null &&
      this.tabs.some((tab) => tab.id === workflow.selectedTabId)
        ? workflow.selectedTabId
        : null;
    const messageId =
      workflow.selectedMessageId !== null &&
      this.messages.some((message) => message.id === workflow.selectedMessageId)
        ? workflow.selectedMessageId
        : null;
    if (!this.draftsOpen && messageId !== null) {
      void this.selectMessage(messageId);
    }
  }

  async openFromNotification(spaceId: number, messageId: number): Promise<void> {
    if (this.activeSpaceId !== spaceId) {
      await this.switchSpace(spaceId);
    }
    await this.selectMessage(messageId);
  }

  async switchSpace(spaceId: number): Promise<void> {
    if (!isTauri() || spaceId === this.activeSpaceId) return;
    if (!this.spaces.some((space) => space.id === spaceId)) return;
    this.spaceSwitching = true;
    try {
      this.saveCurrentWorkflow();
      this.search = "";
      clearTimeout(this.searchTimer);
      this.clearSelection();
      const workflow = loadSpaceWorkflow(spaceId);
      this.activeSpaceId = spaceId;
      setActiveSpaceId(spaceId);
      if (this.spaceAttention[spaceId]) {
        const next = { ...this.spaceAttention };
        delete next[spaceId];
        this.spaceAttention = next;
      }
      this.selectedFolderId = null;
      this.selectedTabId = null;
      this.unified = workflow.unified;
      this.draftsOpen = false;
      this.resetPagination();
      // Let the fade cover the content swap before loading the new space.
      const fadeDelay = loadPreferences().reduce_motion ? 0 : 180;
      await new Promise((resolve) => setTimeout(resolve, fadeDelay));
      if (
        isTauri() &&
        loadSpaceAppearance(spaceId).theme_choice === "matugen"
      ) {
        try {
          const palette = await api.reloadTheme();
          if (palette) applyMatugenPalette(palette);
        } catch {
          // Keep the stored Matugen palette when it cannot be reloaded.
        }
      }
      await appearance.load();
      await this.refresh();
      this.restoreWorkflow(workflow);
      if (this.draftsOpen) await this.loadDrafts();
      else await this.reloadMessages();
      try {
        await api.setActiveThemeSpace(spaceId);
      } catch {
        // Theme persistence is best effort; the local appearance still applies.
      }
      await this.refreshAiStatus();
      await new Promise((resolve) => setTimeout(resolve, 16));
    } finally {
      this.spaceSwitching = false;
    }
  }

  async selectFolder(folderId: number | null): Promise<void> {
    this.clearSelection();
    this.draftsOpen = false;
    this.search = "";
    this.selectedFolderId = folderId;
    this.unified = "inbox";
    this.resetPagination();
    this.saveCurrentWorkflow();
    await this.reloadMessageScope();
  }

  async selectUnified(unified: "inbox" | "sent" | "archive" | "spam" | "trash" | "starred"): Promise<void> {
    this.clearSelection();
    this.draftsOpen = false;
    this.search = "";
    this.selectedFolderId = null;
    this.selectedTabId = null;
    this.unified = unified;
    this.resetPagination();
    this.saveCurrentWorkflow();
    await this.reloadMessageScope();
  }

  async selectTab(tabId: number | null): Promise<void> {
    this.selectedTabId = tabId;
    this.draftsOpen = false;
    this.search = "";
    this.resetPagination();
    this.saveCurrentWorkflow();
    await this.reloadMessageScope();
  }

  async refreshTabs(): Promise<void> {
    if (!isTauri()) return;
    try {
      this.tabs = await api.listTabs(this.activeSpaceId ?? undefined);
      if (
        this.selectedTabId !== null &&
        !this.tabs.some((tab) => tab.id === this.selectedTabId)
      ) {
        this.selectedTabId = null;
      }
    } catch (error) {
      this.error = errorMessage(error);
    }
  }

  async createTab(
    name: string,
    description: string,
    color?: string | null,
    icon?: string | null
  ): Promise<string | null> {
    if (!isTauri()) return null;
    try {
      await api.createTab(
        name,
        description,
        color,
        icon,
        this.activeSpaceId ?? undefined
      );
      this.selectedTabId = null;
      await this.refreshTabs();
      await this.reloadMessages();
      this.error = null;
      return null;
    } catch (error) {
      return errorMessage(error);
    }
  }

  async updateTab(
    id: number,
    name: string,
    description: string,
    color?: string | null,
    icon?: string | null
  ): Promise<string | null> {
    if (!isTauri()) return null;
    try {
      await api.updateTab(
        id,
        name,
        description,
        color,
        icon,
        this.activeSpaceId ?? undefined
      );
      await this.refreshTabs();
      await this.reloadMessages();
      this.error = null;
      return null;
    } catch (error) {
      return errorMessage(error);
    }
  }

  async deleteTab(id: number): Promise<string | null> {
    if (!isTauri()) return null;
    try {
      await api.deleteTab(id, this.activeSpaceId ?? undefined);
      if (this.selectedTabId === id) this.selectedTabId = null;
      await this.refreshTabs();
      await this.reloadMessages();
      this.error = null;
      return null;
    } catch (error) {
      return errorMessage(error);
    }
  }

  async reorderTabs(ids: number[]): Promise<string | null> {
    if (!isTauri()) return null;
    try {
      this.tabs = await api.reorderTabs(
        ids,
        this.activeSpaceId ?? undefined
      );
      this.error = null;
      return null;
    } catch (error) {
      return errorMessage(error);
    }
  }

  async moveTab(id: number, direction: -1 | 1): Promise<string | null> {
    const index = this.tabs.findIndex((tab) => tab.id === id);
    const target = index + direction;
    if (index < 0 || target < 0 || target >= this.tabs.length) return null;
    const ids = this.tabs.map((tab) => tab.id);
    [ids[index], ids[target]] = [ids[target], ids[index]];
    return this.reorderTabs(ids);
  }

  private flashMessage(id: number): void {
    const next = new Set(this.flashingMessageIds);
    next.add(id);
    this.flashingMessageIds = next;
    const existing = this.flashTimers.get(id);
    if (existing) clearTimeout(existing);
    const timer = setTimeout(() => {
      const current = new Set(this.flashingMessageIds);
      current.delete(id);
      this.flashingMessageIds = current;
      this.flashTimers.delete(id);
    }, 1200);
    this.flashTimers.set(id, timer);
  }

  private setClassifying(ids: number[], active: boolean): void {
    const next = new Set(this.classifyingMessageIds);
    for (const id of ids) {
      if (active) next.add(id);
      else next.delete(id);
    }
    this.classifyingMessageIds = next;
  }

  handleClassificationStarted(ids: number[]): void {
    if (ids.length > 0) this.setClassifying(ids, true);
  }

  async handleClassificationDone(payload: ClassificationDone): Promise<void> {
    const ids = payload.results.map((result) => result.message_id);
    this.setClassifying(ids, false);
    for (const result of payload.results) {
      if (result.classification) {
        this.applyClassification(result.message_id, result.classification);
        if (result.classification.tab_id !== null) this.flashMessage(result.message_id);
      }
    }
    await this.refreshTabCounts();
    if (payload.classified > 0) {
      this.setClassificationNotice(
        `Classified ${payload.classified} message${payload.classified === 1 ? "" : "s"}.`
      );
    }
  }

  private applyClassification(messageId: number, classification: Classification): void {
    this.invalidateMessageCache();
    this.messages = this.messages.map((message) =>
      message.id === messageId
        ? { ...message, tab_id: classification.tab_id }
        : message
    );
    if (this.selectedMessage?.message.id === messageId) {
      this.selectedMessage = {
        ...this.selectedMessage,
        message: { ...this.selectedMessage.message, tab_id: classification.tab_id }
      };
    }
  }

  private beginClassificationProgress(total: number, abortable: boolean): void {
    this.classificationAbort = false;
    clearTimeout(this.noticeTimer);
    this.classificationNotice = null;
    this.classificationAction = null;
    this.classificationProgress = {
      total,
      done: 0,
      aborting: false,
      abortable
    };
  }

  private updateClassificationProgress(done: number): void {
    if (!this.classificationProgress) return;
    this.classificationProgress = { ...this.classificationProgress, done };
  }

  abortClassification(): void {
    if (!this.classificationProgress?.abortable || this.classificationProgress.aborting) {
      return;
    }
    this.classificationAbort = true;
    this.classificationProgress = {
      ...this.classificationProgress,
      aborting: true
    };
  }

  private finishClassificationProgress(
    classified: number,
    total: number,
    aborted: boolean
  ): void {
    this.classificationProgress = null;
    this.classificationAbort = false;
    if (classified > 0) {
      const noun = classified === 1 ? "message" : "messages";
      const totalNoun = total === 1 ? "message" : "messages";
      this.setClassificationNotice(
        aborted
          ? `Classified ${classified} of ${total} ${totalNoun} before stopping.`
          : `Classified ${classified} ${noun}.`
      );
    } else if (aborted) {
      this.setClassificationNotice("Classification stopped.");
    }
  }

  private async classifyInSequence(ids: number[]): Promise<ClassificationOutcome[]> {
    if (ids.length === 0) return [];
    const total = ids.length;
    this.beginClassificationProgress(total, total > 1);
    const outcomes: ClassificationOutcome[] = [];
    let classified = 0;
    try {
      for (const id of ids) {
        if (this.classificationAbort) break;
        this.setClassifying([id], true);
        let tabId: number | null = null;
        try {
          const classification = await api.classifyMessage(id, true);
          tabId = classification.tab_id;
          classified += 1;
          outcomes.push({
            message_id: id,
            classification,
            error: null,
            error_kind: null
          });
        } catch (error) {
          outcomes.push({
            message_id: id,
            classification: null,
            error: errorMessage(error),
            error_kind: null
          });
        } finally {
          this.setClassifying([id], false);
        }
        try {
          await this.reloadMessages();
        } catch (error) {
          this.error = errorMessage(error);
        }
        if (tabId !== null) this.flashMessage(id);
        this.updateClassificationProgress(outcomes.length);
      }
    } finally {
      this.finishClassificationProgress(classified, total, this.classificationAbort);
    }
    return outcomes;
  }

  async classifySelected(): Promise<void> {
    if (!isTauri() || !this.selectedMessage || this.classifyBusy) return;
    this.classifyBusy = true;
    try {
      const outcomes = await this.classifyInSequence([
        this.selectedMessage.message.id
      ]);
      this.error = outcomes.find((outcome) => outcome.error)?.error ?? null;
    } catch (error) {
      this.error = errorMessage(error);
    } finally {
      this.classifyBusy = false;
    }
  }

  async classifyVisible(): Promise<void> {
    if (!isTauri() || this.classifyBusy || this.messages.length === 0) return;
    this.classifyBusy = true;
    const ids = this.messages.map((message) => message.id);
    try {
      await this.classifyInSequence(ids);
      this.error = null;
    } catch (error) {
      this.error = errorMessage(error);
    } finally {
      this.classifyBusy = false;
    }
  }

  async classifyUnclassified(): Promise<void> {
    if (!isTauri() || this.classifyBusy) return;
    this.classifyBusy = true;
    try {
      const folder = this.folders.find(
        (candidate) => candidate.id === this.selectedFolderId
      );
      const [classified, failed] = await api.classifyUnclassified(
        folder?.account_id,
        this.selectedFolderId ?? undefined,
        this.activeSpaceId ?? undefined
      );
      void classified;
      void failed;
      await this.reloadMessages();
      this.error = null;
    } catch (error) {
      this.error = errorMessage(error);
    } finally {
      this.classifyBusy = false;
    }
  }

  setClassificationNotice(
    message: string | null,
    action: "reclassify" | null = null
  ): void {
    clearTimeout(this.noticeTimer);
    this.classificationNotice = message;
    this.classificationAction = message ? action : null;
    if (message) {
      this.noticeTimer = setTimeout(() => {
        this.classificationNotice = null;
        this.classificationAction = null;
      }, 6000);
    }
  }

  setThemeNotice(message: string | null): void {
    clearTimeout(this.themeTimer);
    this.themeNotice = message;
    if (message) {
      this.themeTimer = setTimeout(() => {
        this.themeNotice = null;
      }, 6000);
    }
  }

  async selectDrafts(): Promise<void> {
    this.clearSelection();
    this.draftsOpen = true;
    this.search = "";
    this.selectedFolderId = null;
    this.resetPagination();
    this.saveCurrentWorkflow();
    await this.refresh();
  }

  async openCompose(
    mode: ComposeMode,
    source: MessageDetail | null = null,
    draft: Draft | null = null
  ): Promise<boolean> {
    this.composerGeneration += 1;
    this.composerMode = mode;
    this.composerSource = source;
    this.composerDraft = draft;
    this.composerAiDraft = null;
    this.composerAiError = null;
    this.composerAiDrafting = false;
    this.composerOpen = true;
    if (!isTauri()) return true;
    try {
      await api.openComposeWindow(
        mode,
        source?.message.id ?? null,
        draft?.id ?? null
      );
      return true;
    } catch (error) {
      this.composerOpen = false;
      this.error = errorMessage(error);
      return false;
    }
  }

  private async ensureAccountSpace(accountId: number | null): Promise<void> {
    if (accountId === null) return;
    const account = this.accounts.find((candidate) => candidate.id === accountId);
    if (account && account.space_id !== this.activeSpaceId) {
      await this.switchSpace(account.space_id);
    }
  }

  async openDraft(draftId: number): Promise<boolean> {
    if (!isTauri()) return true;
    try {
      const draft = await api.getDraft(draftId);
      await this.ensureAccountSpace(draft.accountId);
      const opened = await this.openCompose("new", null, draft);
      if (opened) {
        this.error = null;
      } else if (!this.error) {
        this.error = "Could not open the compose window.";
      }
      return opened;
    } catch (error) {
      this.error = errorMessage(error);
      return false;
    }
  }

  closeCompose(): void {
    this.composerGeneration += 1;
    this.composerOpen = false;
    this.composerSource = null;
    this.composerDraft = null;
    this.composerAiDraft = null;
    this.composerAiError = null;
    this.composerAiDrafting = false;
  }

  async loadDrafts(): Promise<void> {
    if (!isTauri()) return;
    try {
      this.drafts = await api.listDrafts(undefined, this.activeSpaceId ?? undefined);
    } catch {
      return;
    }
  }

  async discardDraft(draftId: number): Promise<boolean> {
    if (!isTauri()) return false;
    try {
      await api.deleteDraft(draftId);
      await this.loadDrafts();
      this.error = null;
      return true;
    } catch (error) {
      this.error = errorMessage(error);
      return false;
    }
  }

  async afterSent(accountId?: number): Promise<void> {
    this.closeCompose();
    if (accountId) {
      await api.syncAccount(accountId).catch(() => {});
    }
    await this.refresh();
  }

  clearSelection(): void {
    this.selectionRequest += 1;
    this.clearMarkReadTimer();
    this.auraRailOpen = false;
    this.selectedMessageId = null;
    this.selectedMessage = null;
    this.threadMessages = [];
    this.messageLoading = false;
    this.attachmentNotice = null;
  }

  private clearMarkReadTimer(): void {
    if (this.markReadTimer !== undefined) {
      clearTimeout(this.markReadTimer);
      this.markReadTimer = undefined;
    }
  }

  private scheduleMarkRead(messageId: number): void {
    this.clearMarkReadTimer();
    const delay = loadPreferences().mark_read_delay;
    if (delay === 0) {
      void this.markRead(true, true);
      return;
    }
    if (delay < 0) return;
    this.markReadTimer = setTimeout(() => {
      this.markReadTimer = undefined;
      if (this.selectedMessage?.message.id === messageId) {
        void this.markRead(true, true);
      }
    }, delay * 1000);
  }

  async selectMessage(messageId: number): Promise<void> {
    if (!isTauri()) return;
    this.clearMarkReadTimer();
    const request = ++this.selectionRequest;
    const cached = this.messages.some(
      (message) => message.id === messageId && message.body_cached
    );
    this.selectedMessageId = messageId;
    this.saveCurrentWorkflow();
    this.threadMessages = [];
    if (!cached) this.selectedMessage = null;
    this.messageLoading = !cached;
    this.attachmentNotice = null;
    try {
      const detail = await api.getMessage(messageId);
      if (request !== this.selectionRequest) return;
      this.selectedMessage = detail;
      this.error = null;
      try {
        const thread = await api.listThreadMessages(
          detail.message.account_id,
          detail.message.thread_key
        );
        if (request === this.selectionRequest) this.threadMessages = thread;
      } catch {
        if (request === this.selectionRequest) this.threadMessages = [];
      }
      if (request === this.selectionRequest && !detail.message.is_seen) {
        this.scheduleMarkRead(detail.message.id);
      }
    } catch (error) {
      if (request !== this.selectionRequest) return;
      this.selectedMessage = null;
      this.error = errorMessage(error);
    } finally {
      if (request === this.selectionRequest) this.messageLoading = false;
    }
  }

  async markRead(read: boolean, silent = false): Promise<void> {
    const current = this.selectedMessage;
    if (!isTauri() || !current) return;
    try {
      const updated = await api.markRead(current.message.id, read);
      this.applyMessage(updated);
      await this.refreshFolders();
      await this.refreshTabCounts();
      this.error = null;
    } catch (error) {
      if (!silent) this.error = errorMessage(error);
    }
  }

  async toggleStar(): Promise<void> {
    if (!isTauri() || !this.selectedMessage) return;
    try {
      const updated = await api.toggleStar(this.selectedMessage.message.id);
      this.applyMessage(updated);
      await this.refreshTabCounts();
      this.error = null;
    } catch (error) {
      this.error = errorMessage(error);
    }
  }

  async composeMessage(messageId: number, mode: ComposeMode): Promise<void> {
    if (!isTauri()) return;
    try {
      const detail = await api.getMessage(messageId);
      await this.ensureAccountSpace(detail.message.account_id);
      this.openCompose(mode, detail);
      this.error = null;
    } catch (error) {
      this.error = errorMessage(error);
    }
  }

  async replyToMessage(messageId: number): Promise<void> {
    await this.composeMessage(messageId, "reply");
  }

  async moveMessageToFolder(messageId: number, folderId: number): Promise<void> {
    if (!isTauri()) return;
    await this.queueMessageAction([messageId], () =>
      api.moveMessageToFolder(messageId, folderId)
    );
  }

  async moveMessagesToFolder(ids: number[], folderId: number): Promise<void> {
    if (!isTauri() || ids.length === 0) return;
    await this.queueMessageAction(ids, async () => {
      await Promise.all(ids.map((id) => api.moveMessageToFolder(id, folderId)));
    });
  }

  async categorizeMessage(messageId: number, tabId: number | null): Promise<void> {
    if (!isTauri()) return;
    try {
      const classification = await api.setMessageClassification(messageId, tabId);
      this.applyClassification(messageId, classification);
      await this.reloadMessages();
      if (classification.tab_id !== null) this.flashMessage(messageId);
      this.error = null;
    } catch (error) {
      this.error = errorMessage(error);
    }
  }

  async categorizeMessages(ids: number[], tabId: number | null): Promise<void> {
    if (!isTauri() || ids.length === 0) return;
    try {
      const results = await Promise.allSettled(
        ids.map((id) => api.setMessageClassification(id, tabId))
      );
      const classifiedIds = new Set<number>();
      for (const [index, result] of results.entries()) {
        if (result.status === "fulfilled") {
          classifiedIds.add(ids[index]!);
          this.applyClassification(ids[index]!, result.value);
        }
      }
      await this.reloadMessages();
      if (tabId !== null) {
        for (const id of classifiedIds) this.flashMessage(id);
      }
      const failure = results.find((result) => result.status === "rejected");
      this.error =
        failure?.status === "rejected" ? errorMessage(failure.reason) : null;
    } catch (error) {
      this.error = errorMessage(error);
    }
  }

  async toggleStarMessage(messageId: number): Promise<void> {
    if (!isTauri()) return;
    try {
      const updated = await api.toggleStar(messageId);
      this.applyMessage(updated);
      this.error = null;
    } catch (error) {
      this.error = errorMessage(error);
    }
  }

  async classifyMessageById(messageId: number): Promise<void> {
    if (!isTauri() || this.classifyBusy) return;
    this.classifyBusy = true;
    try {
      const outcomes = await this.classifyInSequence([messageId]);
      this.error = outcomes.find((outcome) => outcome.error)?.error ?? null;
    } catch (error) {
      this.error = errorMessage(error);
    } finally {
      this.classifyBusy = false;
    }
  }

  async classifyMessagesByIds(ids: number[]): Promise<void> {
    if (!isTauri() || this.classifyBusy || ids.length === 0) return;
    this.classifyBusy = true;
    try {
      await this.classifyInSequence(ids);
      this.error = null;
    } catch (error) {
      this.error = errorMessage(error);
    } finally {
      this.classifyBusy = false;
    }
  }

  private messageFolderRole(messageId: number): string | null {
    const message = this.messages.find((candidate) => candidate.id === messageId);
    if (!message) return null;
    return this.folders.find((folder) => folder.id === message.folder_id)?.role ?? null;
  }

  async archiveThread(accountId: number, threadKey: string): Promise<void> {
    if (!isTauri()) return;
    const ids = this.messages
      .filter(
        (message) =>
          message.account_id === accountId && message.thread_key === threadKey
      )
      .map((message) => message.id);
    await this.queueMessageAction(ids, () => api.archiveThread(accountId, threadKey));
  }

  async archiveMessageById(messageId: number): Promise<void> {
    if (!isTauri()) return;
    await this.queueMessageAction([messageId], () => api.archiveMessage(messageId));
  }

  async deleteMessageById(messageId: number): Promise<void> {
    if (!isTauri()) return;
    const permanent = this.messageFolderRole(messageId) === "trash";
    await this.queueMessageAction([messageId], () =>
      permanent ? api.deleteMessage(messageId) : api.trashMessage(messageId)
    );
  }

  async markMessages(ids: number[], read: boolean): Promise<void> {
    if (!isTauri() || ids.length === 0) return;
    try {
      await Promise.all(ids.map((id) => api.markRead(id, read)));
      await this.reloadMessages();
      await this.refreshFolders();
      this.error = null;
    } catch (error) {
      this.error = errorMessage(error);
    }
  }

  async archiveMessages(ids: number[]): Promise<void> {
    if (!isTauri() || ids.length === 0) return;
    const threads = new Map<string, { accountId: number; threadKey: string }>();
    for (const id of ids) {
      const message = this.messages.find((candidate) => candidate.id === id);
      if (message) {
        threads.set(`${message.account_id}:${message.thread_key}`, {
          accountId: message.account_id,
          threadKey: message.thread_key
        });
      }
    }
    const affected = this.messages
      .filter((message) =>
        threads.has(`${message.account_id}:${message.thread_key}`)
      )
      .map((message) => message.id);
    await this.queueMessageAction(affected, async () => {
      await Promise.all(
        [...threads.values()].map((thread) =>
          api.archiveThread(thread.accountId, thread.threadKey)
        )
      );
    });
  }

  async deleteMessages(ids: number[]): Promise<void> {
    if (!isTauri() || ids.length === 0) return;
    const permanent = new Set(
      ids.filter((id) => this.messageFolderRole(id) === "trash")
    );
    await this.queueMessageAction(ids, async () => {
      await Promise.all(
        ids.map((id) =>
          permanent.has(id) ? api.deleteMessage(id) : api.trashMessage(id)
        )
      );
    });
  }

  async trashSelected(): Promise<void> {
    if (!isTauri() || !this.selectedMessage) return;
    const messageId = this.selectedMessage.message.id;
    await this.queueMessageAction([messageId], () => api.trashMessage(messageId));
  }

  async deleteSelected(): Promise<void> {
    if (!isTauri() || !this.selectedMessage) return;
    const messageId = this.selectedMessage.message.id;
    await this.queueMessageAction([messageId], () => api.deleteMessage(messageId));
  }

  async downloadAttachment(attachmentId: number): Promise<string | null> {
    if (!isTauri()) return null;
    this.attachmentNotice = null;
    try {
      const path = await api.downloadAttachment(attachmentId);
      this.attachmentNotice = `Saved to ${path}`;
      this.error = null;
      if (loadPreferences().reveal_download) {
        void api.revealDownload(path).catch(() => {});
      }
      return path;
    } catch (error) {
      this.attachmentNotice = null;
      this.error = errorMessage(error);
      return null;
    }
  }

  private rememberMessages(key: string, threads: ThreadMessage[]): void {
    this.messageCache.delete(key);
    this.messageCache.set(key, threads);
    while (this.messageCache.size > 12) {
      const oldest = this.messageCache.keys().next().value;
      if (oldest === undefined) break;
      this.messageCache.delete(oldest);
    }
  }

  private invalidateMessageCache(): void {
    this.messageCache.clear();
  }

  private applyMessages(
    threads: ThreadMessage[],
    request: number,
    progressive: boolean
  ): void {
    if (!progressive) {
      this.renderRequest += 1;
      this.progressiveRenderActive = false;
      this.messages = threads;
      return;
    }
    const render = ++this.renderRequest;
    this.progressiveRenderActive = threads.length > 40;
    this.messages = threads.slice(0, 40);
    if (threads.length > 40) void this.appendMessages(threads, request, render);
  }

  private async appendMessages(
    threads: ThreadMessage[],
    request: number,
    render: number
  ): Promise<void> {
    for (let start = 40; start < threads.length; start += 40) {
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      if (request !== this.messageRequest || render !== this.renderRequest) return;
      // A refresh or message action may have replaced the list while this
      // batch was scheduled. Never append stale rows onto a newer list.
      if (this.messages.length !== start) {
        this.progressiveRenderActive = false;
        return;
      }
      this.messages = this.messages.concat(threads.slice(start, start + 40));
    }
    if (render === this.renderRequest) this.progressiveRenderActive = false;
  }

  private sameMessages(a: ThreadMessage[], b: ThreadMessage[]): boolean {
    return JSON.stringify(a) === JSON.stringify(b);
  }

  private async loadMessages(progressive: boolean): Promise<void> {
    if (!isTauri()) return;
    const query = this.messageQuery();
    const key = JSON.stringify(query);
    const cached = this.messageCache.get(key);
    const cachedVisible = cached ? this.filterHidden(cached) : null;
    const request = ++this.messageRequest;
    if (cachedVisible && progressive) {
      this.applyMessages(cachedVisible, request, true);
    }

    const threads = await api.listThreads(query);
    if (request !== this.messageRequest) return;
    const visible = this.filterHidden(threads);
    this.rememberMessages(key, threads);
    if (!cachedVisible || !this.sameMessages(cachedVisible, visible)) {
      this.applyMessages(visible, request, progressive);
    }
  }

  async reloadMessages(): Promise<void> {
    this.invalidateMessageCache();
    await this.loadMessages(false);
    await this.refreshTabCounts();
  }

  private scheduleUnifiedPrefetch(): void {
    if (!isTauri() || this.activeSpaceId === null || this.searchActive) return;
    const spaceId = this.activeSpaceId;
    const order = loadPreferences().message_order;
    clearTimeout(this.prefetchTimer);
    this.prefetchTimer = setTimeout(() => {
      void this.prefetchUnifiedViews(spaceId, order);
    }, 900);
  }

  private async prefetchUnifiedViews(
    spaceId: number,
    order: MessageQuery["order"]
  ): Promise<void> {
    for (const unified of ["inbox", "archive", "spam"] as const) {
      if (this.activeSpaceId !== spaceId || this.searchActive) return;
      const query: MessageQuery = {
        space_id: spaceId,
        unified,
        limit: 200,
        order
      };
      const key = JSON.stringify(query);
      if (this.messageCache.has(key)) continue;
      try {
        const threads = await api.listThreads(query);
        if (this.activeSpaceId !== spaceId || this.searchActive) return;
        this.rememberMessages(key, threads);
      } catch {
        return;
      }
    }
  }

  /**
   * Changing an inbox/folder/tab is a message-list operation. The surrounding
   * account, folder, draft, and tab metadata does not change, so do not make
   * the new list wait for a full application refresh.
   */
  private async reloadMessageScope(): Promise<void> {
    this.messageScopeLoading = true;
    try {
      await this.loadMessages(true);
      this.error = null;
    } catch (error) {
      this.error = errorMessage(error);
    } finally {
      this.messageScopeLoading = false;
    }
    void this.refreshTabCounts();
  }

  private async refreshFolders(): Promise<void> {
    try {
      this.folders = await api.listFolders(
        undefined,
        this.activeSpaceId ?? undefined
      );
      this.refreshTrayUnread();
    } catch {
      return;
    }
  }

  private refreshTrayUnread(): void {
    void Promise.resolve(api.refreshTrayUnread()).catch(() => {});
  }

  private filterHidden<T extends Message>(messages: T[]): T[] {
    if (this.hiddenMessageIds.size === 0) return messages;
    return messages.filter((message) => !this.hiddenMessageIds.has(message.id));
  }

  private async queueMessageAction(ids: number[], run: () => Promise<void>): Promise<void> {
    const unique = [...new Set(ids)];
    if (unique.length === 0) return;
    this.invalidateMessageCache();
    for (const id of unique) this.hiddenMessageIds.add(id);
    this.messages = this.filterHidden(this.messages);
    if (this.selectedMessageId !== null && this.hiddenMessageIds.has(this.selectedMessageId)) {
      this.clearSelection();
    }
    const item = { ids: unique, run };
    if (this.syncing || this.flushingActions) {
      this.queuedActions.push(item);
      return;
    }
    await this.runQueuedAction(item);
  }

  private async runQueuedAction(item: { ids: number[]; run: () => Promise<void> }): Promise<void> {
    try {
      await item.run();
      this.error = null;
    } catch (error) {
      this.error = errorMessage(error);
    } finally {
      for (const id of item.ids) this.hiddenMessageIds.delete(id);
      try {
        await this.reloadMessages();
      } catch (error) {
        this.error = errorMessage(error);
      }
      try {
        await this.refreshFolders();
      } catch {
        // Keep the completed action even if the folder refresh fails.
      }
    }
  }

  async flushQueuedActions(): Promise<void> {
    if (this.flushingActions || this.syncing) return;
    this.flushingActions = true;
    try {
      while (this.queuedActions.length > 0 && !this.syncing) {
        const item = this.queuedActions.shift();
        if (item) await this.runQueuedAction(item);
      }
    } finally {
      this.flushingActions = false;
    }
  }

  private applyMessage(updated: Message): void {
    this.invalidateMessageCache();
    this.messages = this.messages.map((message) =>
      message.id === updated.id ? { ...message, ...updated } : message
    );
    if (this.selectedMessage?.message.id === updated.id) {
      this.selectedMessage = { ...this.selectedMessage, message: updated };
    }
  }

  private removeMessage(messageId: number): void {
    this.invalidateMessageCache();
    this.messages = this.messages.filter((message) => message.id !== messageId);
    if (this.selectedMessageId === messageId) {
      this.selectedMessageId = null;
      this.selectedMessage = null;
      this.threadMessages = [];
    }
  }

  setSearch(value: string): void {
    this.search = value;
    this.resetPagination();
    clearTimeout(this.searchTimer);
    this.searchTimer = setTimeout(() => void this.refresh(), 200);
  }

  async fillVisibleSnippets(): Promise<void> {
    if (!isTauri()) return;
    const ids = this.messages
      .filter(
        (message) =>
          !message.snippet.trim() && !this.snippetAttempts.has(message.id)
      )
      .slice(0, 80)
      .map((message) => message.id);
    if (ids.length === 0) return;
    for (const id of ids) this.snippetAttempts.add(id);
    try {
      const snippets = await api.fillMessageSnippets(ids);
      if (snippets.length === 0) return;
      const byId = new Map(
        snippets.map((snippet) => [snippet.message_id, snippet.snippet])
      );
      this.messages = this.messages.map((message) =>
        byId.has(message.id)
          ? { ...message, snippet: byId.get(message.id) ?? "" }
          : message
      );
    } catch {
      for (const id of ids) this.snippetAttempts.delete(id);
      return;
    }
  }

  async syncAll(): Promise<void> {
    if (!isTauri() || this.accounts.length === 0) return;
    await Promise.allSettled(
      this.accounts.map((account) => api.syncAccount(account.id))
    );
    await this.refresh();
  }

  async refreshAll(): Promise<void> {
    await this.syncAll();
  }

  destroy(): void {
    clearTimeout(this.searchTimer);
    clearTimeout(this.noticeTimer);
    clearTimeout(this.themeTimer);
    clearTimeout(this.prefetchTimer);
    this.messageCache.clear();
    this.clearMarkReadTimer();
    for (const timer of this.flashTimers.values()) clearTimeout(timer);
    this.flashTimers.clear();
    for (const unlisten of this.unlisteners) unlisten();
    this.unlisteners = [];
    this.initialized = false;
  }

  private async bindEvents(): Promise<void> {
    const status = await listen<SyncStatus>("sync://status", ({ payload }) => {
      this.statuses = { ...this.statuses, [payload.accountId]: payload };
      if (payload.state !== "syncing") void this.flushQueuedActions();
    });
    const folderStatus = await listen<SyncFolderStatus>("sync://folder", ({ payload }) => {
      const next = { ...this.syncingFolders };
      if (payload.syncing) next[payload.folderId] = true;
      else delete next[payload.folderId];
      this.syncingFolders = next;
    });
    const created = await listen<{ accountId: number; inserted: number }>(
      "sync://new",
      ({ payload }) => {
        if (payload?.inserted > 0 && typeof payload.accountId === "number") {
          this.noteSpaceActivity(payload.accountId);
        }
        void this.refresh();
      }
    );
    const composeChanged = await listen("compose://changed", () => {
      this.closeCompose();
      void this.loadDrafts();
      void this.reloadMessages();
    });
    const composeSent = await listen<number>("compose://sent", ({ payload }) => {
      void this.afterSent(payload);
    });
    const classificationStarted = await listen<{
      accountId: number;
      messageIds: number[];
    }>("classification://started", ({ payload }) => {
      this.handleClassificationStarted(payload.messageIds);
    });
    const classified = await listen<ClassificationDone>(
      "classification://done",
      ({ payload }) => {
        void this.handleClassificationDone(payload);
      }
    );
    const themeChanged = await listen("theme://changed", ({ payload }) => {
      try {
        applyMatugenPalette(payload);
      } catch {
        this.setThemeNotice("Matugen theme is invalid; keeping the current theme.");
      }
    });
    const themeError = await listen<string>("theme://error", ({ payload }) => {
      this.setThemeNotice(payload);
    });
    const spaceOpen = await listen<{
      spaceId?: number;
      space_id?: number;
      messageId?: number;
      message_id?: number;
    }>("space://open", ({ payload }) => {
      const spaceId = payload.spaceId ?? payload.space_id;
      const messageId = payload.messageId ?? payload.message_id;
      if (spaceId != null && messageId != null) {
        void this.openFromNotification(spaceId, messageId);
      }
    });
    const protonAccessLimited = await listen<ProtonAccessLimitedEvent>(
      "proton://access_limited",
      ({ payload }) => {
        this.protonAccessLimited = payload;
      }
    );
    this.unlisteners.push(
      status,
      folderStatus,
      created,
      composeChanged,
      composeSent,
      classificationStarted,
      classified,
      themeChanged,
      themeError,
      spaceOpen,
      protonAccessLimited
    );
  }
}

export const app = new AppStore();
