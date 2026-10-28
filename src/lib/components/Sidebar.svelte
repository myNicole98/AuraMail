<script lang="ts">
  import {
    Archive,
    Ban,
    ChevronDown,
    ChevronRight,
    FileText,
    Folder,
    Inbox,
    PanelLeftClose,
    PanelLeftOpen,
    RefreshCw,
    Send,
    Star,
    Trash2
  } from "lucide-svelte";
  import { accountColorValue } from "../accountColors";
  import { app } from "../stores/app.svelte";
  import { appearance } from "../stores/appearance.svelte";
  import SpaceSelector from "./SpaceSelector.svelte";

  let {
    collapsed,
    canToggle,
    onToggle
  }: {
    collapsed: boolean;
    canToggle: boolean;
    onToggle: () => void;
  } = $props();

  let accounts = $derived(app.activeAccounts);
  let folders = $derived(app.activeFolders);
  let unifiedCount = $derived(
    folders.filter((folder) => folder.role === "inbox").reduce((sum, folder) => sum + folder.unread_count, 0)
  );
  let spamCount = $derived(
    folders.filter((folder) => folder.role === "junk").reduce((sum, folder) => sum + folder.unread_count, 0)
  );
  let grouped = $derived(
    accounts.map((account) => ({
      account,
      color: accountColorValue(account.color, appearance.colorVision, appearance.themeDark),
      folders: folders.filter(
        (folder) => folder.account_id === account.id && folder.role !== "drafts"
      )
    }))
  );
  const ACCOUNT_COLLAPSED_KEY = "auramail.accounts.collapsed";

  function loadCollapsedAccounts(): Record<number, boolean> {
    try {
      const raw = localStorage.getItem(ACCOUNT_COLLAPSED_KEY);
      return raw ? (JSON.parse(raw) as Record<number, boolean>) : {};
    } catch {
      return {};
    }
  }

  let collapsedAccounts = $state<Record<number, boolean>>(loadCollapsedAccounts());
  let draftCount = $derived(app.drafts.length);

  function folderSyncing(folderId: number): boolean {
    return app.syncingFolders[folderId] === true;
  }

  function toggleAccount(accountId: number) {
    collapsedAccounts = {
      ...collapsedAccounts,
      [accountId]: !collapsedAccounts[accountId]
    };
    localStorage.setItem(ACCOUNT_COLLAPSED_KEY, JSON.stringify(collapsedAccounts));
  }
  let statusLabel = $derived(app.syncLabel);
  let statusState = $derived(app.syncState);

  function openAccountSettings() {
    window.dispatchEvent(
      new CustomEvent("auramail:open-settings", {
        detail: { tab: "account" }
      })
    );
  }

  function iconFor(role: string) {
    switch (role) {
      case "inbox":
        return Inbox;
      case "sent":
        return Send;
      case "drafts":
        return FileText;
      case "junk":
        return Ban;
      case "trash":
        return Trash2;
      case "archive":
        return Archive;
      default:
        return Folder;
    }
  }
</script>

<aside class="sidebar" class:collapsed aria-label="Mail navigation">
  <nav class="nav">
    <SpaceSelector {collapsed} onOpenAccountSettings={openAccountSettings} />
    <button
      class="nav-item"
      class:active={app.selectedFolderId === null && !app.draftsOpen && !app.calendarOpen && app.unified === "inbox"}
      type="button"
      aria-label="Inbox"
      title="Inbox"
      onclick={() => void app.selectUnified("inbox")}
    >
      <Inbox size={17} />
      {#if !collapsed}
        <span class="nav-label">Inbox</span>
        {#if unifiedCount > 0}
          <span class="count">{unifiedCount}</span>
        {/if}
      {/if}
    </button>

    <button
      class="nav-item"
      class:active={app.selectedFolderId === null && !app.draftsOpen && !app.calendarOpen && app.unified === "sent"}
      type="button"
      aria-label="Sent"
      title="Sent"
      onclick={() => void app.selectUnified("sent")}
    >
      <Send size={17} />
      {#if !collapsed}
        <span class="nav-label">Sent</span>
      {/if}
    </button>

    <button
      class="nav-item"
      class:active={app.selectedFolderId === null && !app.draftsOpen && !app.calendarOpen && app.unified === "archive"}
      type="button"
      aria-label="Archive"
      title="Archive"
      onclick={() => void app.selectUnified("archive")}
    >
      <Archive size={17} />
      {#if !collapsed}
        <span class="nav-label">Archive</span>
      {/if}
    </button>

    <button
      class="nav-item"
      class:active={app.selectedFolderId === null && !app.draftsOpen && !app.calendarOpen && app.unified === "spam"}
      type="button"
      aria-label="Spam"
      title="Spam"
      onclick={() => void app.selectUnified("spam")}
    >
      <Ban size={17} />
      {#if !collapsed}
        <span class="nav-label">Spam</span>
        {#if spamCount > 0}
          <span class="count">{spamCount}</span>
        {/if}
      {/if}
    </button>

    <button
      class="nav-item"
      class:active={app.selectedFolderId === null && !app.draftsOpen && !app.calendarOpen && app.unified === "trash"}
      type="button"
      aria-label="Trash"
      title="Trash"
      onclick={() => void app.selectUnified("trash")}
    >
      <Trash2 size={17} />
      {#if !collapsed}
        <span class="nav-label">Trash</span>
      {/if}
    </button>

    <button
      class="nav-item"
      class:active={app.selectedFolderId === null && !app.draftsOpen && !app.calendarOpen && app.unified === "starred"}
      type="button"
      aria-label="Starred"
      title="Starred"
      onclick={() => void app.selectUnified("starred")}
    >
      <Star size={17} />
      {#if !collapsed}
        <span class="nav-label">Starred</span>
      {/if}
    </button>

    <button
      class="nav-item"
      class:active={app.draftsOpen && !app.calendarOpen}
      type="button"
      aria-label="Drafts"
      title="Drafts"
      onclick={() => void app.selectDrafts()}
    >
      <FileText size={17} />
      {#if !collapsed}
        <span class="nav-label">Drafts</span>
        {#if draftCount > 0}
          <span class="count subtle">{draftCount}</span>
        {/if}
      {/if}
    </button>

    {#if !collapsed}
    {#each grouped as { account, color, folders: accountFolders } (account.id)}
      <div class="account">
        {#if !collapsed}
          <button
            class="account-head"
            type="button"
            aria-expanded={!collapsedAccounts[account.id]}
            title={`${collapsedAccounts[account.id] ? "Expand" : "Collapse"} ${account.display_name}`}
            onclick={() => toggleAccount(account.id)}
          >
            {#if collapsedAccounts[account.id]}
              <ChevronRight size={12} />
            {:else}
              <ChevronDown size={12} />
            {/if}
            <span class="dot" style:background={color}></span>
            <span class="account-name">{account.display_name}</span>
          </button>
        {/if}
        {#if !collapsedAccounts[account.id]}
          {#each accountFolders as folder (folder.id)}
          {@const Icon = iconFor(folder.role)}
          <button
            class="nav-item sub"
            class:active={app.selectedFolderId === folder.id}
            type="button"
            aria-label={`${account.display_name} ${folder.name}`}
            title={`${account.display_name} · ${folder.name}`}
            onclick={() => void app.selectFolder(folder.id)}
          >
            <Icon size={15} />
            {#if !collapsed}
              <span class="nav-label">{folder.name}</span>
              {#if folderSyncing(folder.id)}
                <span class="sync-icon" title="Syncing"><RefreshCw size={12} /></span>
              {/if}
              {#if folder.unread_count > 0}
                <span class="count subtle">{folder.unread_count}</span>
              {/if}
            {/if}
          </button>
          {/each}
        {/if}
      </div>
    {/each}
    {/if}

    {#if !collapsed && grouped.length === 0}
      <p class="empty">No accounts yet.</p>
    {/if}
  </nav>

  <footer class="sidebar-foot">
    <div class="foot-row">
      <div
        class="status"
        class:syncing={statusState === "syncing"}
        class:offline={statusState === "offline"}
        class:error={statusState === "error"}
        title={statusLabel}
      >
        <span class="status-dot"></span>
        {#if !collapsed}
          <span>{statusLabel}</span>
        {/if}
      </div>
      <button
        class="icon-btn sync-btn"
        type="button"
        aria-label="Sync now"
        title="Sync now"
        disabled={statusState === "syncing"}
        onclick={() => void app.refreshAll()}
      >
        <RefreshCw size={15} />
      </button>
      <div class="foot-actions">
        <button
          class="icon-btn toggle"
          type="button"
          onclick={onToggle}
          disabled={!canToggle}
          aria-disabled={!canToggle}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          title={canToggle ? "Toggle sidebar (Ctrl+B)" : "Sidebar auto-collapses below 1100px"}
        >
          {#if collapsed}
            <PanelLeftOpen size={16} />
          {:else}
            <PanelLeftClose size={16} />
          {/if}
        </button>
      </div>
    </div>
  </footer>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    height: 100%;
    padding: 10px 8px;
    overflow: hidden;
    background: var(--bg-raised);
  }
  .sidebar.collapsed {
    align-items: center;
    padding: 10px 5px;
  }
  .toggle {
    flex: none;
  }
  .sidebar.collapsed .toggle {
    width: 28px;
    height: 28px;
  }
  .icon-btn {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex: none;
    border-radius: 7px;
    color: var(--muted);
  }
  .icon-btn:hover:not(:disabled) {
    color: var(--text);
    background: var(--bg-hover);
  }
  .icon-btn:disabled {
    cursor: default;
    opacity: 0.4;
  }
  .nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  .sidebar.collapsed .nav {
    align-items: center;
    width: 100%;
  }
  .nav-item {
    outline: 0;
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    height: 32px;
    padding: 0 9px;
    border-radius: 8px;
    color: var(--muted);
    text-align: left;
  }
  .nav-item:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .nav-item:focus-visible {
    outline: 0;
  }
  .nav-item.active {
    color: var(--text-strong);
    background: color-mix(in srgb, var(--accent) 15%, transparent);
  }
  .nav-item.active :global(svg) {
    color: var(--accent);
  }
  .sidebar.collapsed .nav-item {
    justify-content: center;
    width: 36px;
    padding: 0;
  }
  .nav-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-size: 12.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--bg);
    font-size: 10.5px;
    font-weight: 650;
  }
  .count.subtle {
    background: var(--bg-hover);
    color: var(--muted);
  }
  .account {
    margin-top: 12px;
  }
  .account-head {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 0 9px 5px;
    color: var(--muted);
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-align: left;
    text-transform: uppercase;
  }
  .account-head:hover {
    color: var(--text);
  }
  .account-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sync-icon {
    display: grid;
    place-items: center;
    width: 14px;
    height: 14px;
    flex: none;
    color: var(--accent);
    transform-origin: center;
    animation: spin 1s linear infinite;
  }
  .dot {
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 50%;
  }
  :global([data-color-vision="achromatopsia"]) .dot {
    box-shadow: 0 0 0 1px var(--border);
  }
  .nav-item.sub {
    height: 29px;
    padding-left: 20px;
  }
  .sidebar.collapsed .nav-item.sub {
    justify-content: center;
    width: 36px;
    padding: 0;
  }
  .empty {
    margin: 8px 9px;
    color: var(--muted);
    font-size: 11.5px;
  }
  .sidebar-foot {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex: none;
    padding-top: 8px;
    border-top: 1px solid color-mix(in srgb, var(--border) 45%, transparent);
  }
  .foot-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    min-height: 34px;
  }
  .foot-row .status {
    flex: 1;
  }
  .sidebar.collapsed .foot-row {
    flex-direction: column;
    gap: 5px;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    padding: 0 9px;
    overflow: hidden;
    color: var(--muted);
    font-size: 11px;
    white-space: nowrap;
  }
  .sidebar.collapsed .status {
    justify-content: center;
    padding: 0;
  }
  .status-dot {
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 50%;
    background: var(--success);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--success) 16%, transparent);
  }
  .status.syncing .status-dot {
    background: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 16%, transparent);
    animation: pulse 1.4s ease-in-out infinite;
  }
  .status.offline .status-dot {
    background: var(--warning);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--warning) 16%, transparent);
  }
  .status.error .status-dot {
    background: var(--danger);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--danger) 16%, transparent);
  }
  .foot-actions {
    display: flex;
    gap: 2px;
  }
  .sidebar.collapsed .foot-actions {
    flex-direction: row;
    align-items: center;
    gap: 2px;
  }
  .sidebar.collapsed .foot-actions .icon-btn {
    width: 26px;
    height: 26px;
  }
  @keyframes pulse {
    50% {
      opacity: 0.45;
    }
  }
</style>
