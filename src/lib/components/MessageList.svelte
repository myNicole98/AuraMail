<script lang="ts">
  import { onMount } from "svelte";
  import {
    Archive,
    CircleAlert,
    FileText,
    Paperclip,
    PenLine,
    Reply,
    Search,
    Star,
    Tags,
    Trash2,
    X
  } from "lucide-svelte";
  import { accountColorValue } from "../accountColors";
  import { relativeDate } from "../format";
  import { tabColor, tabIconComponent } from "../tabAppearance";
  import { app } from "../stores/app.svelte";
  import { appearance } from "../stores/appearance.svelte";
  import type { DraftSummary, Message, ThreadMessage } from "../types";
  import Checkbox from "./Checkbox.svelte";
  import ClassificationTabs from "./ClassificationTabs.svelte";
  import ClassificationToast from "./ClassificationToast.svelte";
  import MailContextMenu, { type MailContextAction } from "./MailContextMenu.svelte";

  let messages = $derived(app.messages);
  let drafts = $derived(app.drafts);
  let draftsOpen = $derived(app.draftsOpen);
  let selected = $state<Set<number>>(new Set());
  let selectionAnchor = $state<number | null>(null);
  let bulkBusy = $state(false);
  let contextMenu = $state<{
    x: number;
    y: number;
    message: ThreadMessage;
  } | null>(null);
  let showClassificationIcon = $derived(app.selectedTabId === null && !app.draftsOpen);
  let classificationProgress = $derived(app.classificationProgress);
  let classificationMessage = $derived(
    classificationProgress
      ? `Classified ${classificationProgress.done} of ${classificationProgress.total} message${classificationProgress.total === 1 ? "" : "s"}`
      : (app.classificationNotice ?? "")
  );
  let allSelected = $derived(
    messages.length > 0 && messages.every((message) => selected.has(message.id))
  );

  function toggleAll() {
    const selectAll = !allSelected;
    selected = selectAll ? new Set(messages.map((message) => message.id)) : new Set();
    selectionAnchor = selectAll ? (messages[0]?.id ?? null) : null;
  }

  function onRowClick(event: MouseEvent, messageId: number) {
    const index = messages.findIndex((message) => message.id === messageId);
    if (index < 0) return;

    if (event.shiftKey) {
      event.preventDefault();
      const anchorIndex =
        selectionAnchor === null
          ? -1
          : messages.findIndex((message) => message.id === selectionAnchor);
      if (anchorIndex < 0) {
        selected = new Set([messageId]);
        selectionAnchor = messageId;
        return;
      }
      const start = Math.min(anchorIndex, index);
      const end = Math.max(anchorIndex, index);
      selected = new Set(
        messages.slice(start, end + 1).map((message) => message.id)
      );
      return;
    }

    if (event.ctrlKey || event.metaKey) {
      event.preventDefault();
      const next = new Set(selected);
      const opened = app.selectedMessageId;
      // The mail that is already open is part of the selection in spirit.
      // Include it silently when a new Ctrl-selection starts.
      if (next.size === 0 && opened !== null && opened !== messageId) {
        next.add(opened);
      }
      if (next.has(messageId)) next.delete(messageId);
      else next.add(messageId);
      selected = next;
      selectionAnchor = messageId;
      return;
    }

    selectionAnchor = messageId;
    if (selected.size > 0) selected = new Set();
    void app.selectMessage(messageId);
  }

  async function runBulk(action: (ids: number[]) => Promise<void>) {
    const ids = [...selected];
    if (ids.length === 0) return;
    // Clear the selection as soon as the action starts so the selected
    // rows lose their state while the action is still running.
    selected = new Set();
    selectionAnchor = null;
    bulkBusy = true;
    try {
      await action(ids);
    } finally {
      bulkBusy = false;
    }
  }

  function accountFor(message: Message) {
    return app.accounts.find((account) => account.id === message.account_id);
  }

  $effect(() => {
    const visible = new Set(messages.map((message) => message.id));
    if (selectionAnchor !== null && !visible.has(selectionAnchor)) {
      selectionAnchor = null;
    }
    if ([...selected].every((id) => visible.has(id))) return;
    selected = new Set([...selected].filter((id) => visible.has(id)));
  });

  function draftRecipients(draft: DraftSummary): string {
    return draft.toAddrs.join(", ") || draft.ccAddrs.join(", ") || "(no recipient)";
  }

  function draftSubject(draft: DraftSummary): string {
    return draft.subject.trim() || "(no subject)";
  }

  function draftSnippet(draft: DraftSummary): string {
    return draft.snippet.split(/\s+/).filter(Boolean).join(" ").slice(0, 120);
  }

  function sender(message: Message): string {
    return message.from_name.trim() || message.from_addr || "Unknown sender";
  }

  function subject(message: Message): string {
    return message.subject.trim() || "(no subject)";
  }

  function classificationFor(message: Message) {
    return message.tab_id === null
      ? null
      : app.tabs.find((tab) => tab.id === message.tab_id) ?? null;
  }

  function classificationColor(message: Message): string {
    const tab = classificationFor(message);
    if (!tab) return "var(--accent)";
    const index = app.tabs.findIndex((candidate) => candidate.id === tab.id);
    return tabColor(tab, index, appearance.colorVision, appearance.themeDark);
  }

  function onSearch(event: Event) {
    app.setSearch((event.currentTarget as HTMLInputElement).value);
  }

  function onRowsScroll(event: Event) {
    if (contextMenu !== null) contextMenu = null;
    const element = event.currentTarget as HTMLElement;
    if (element.scrollHeight - element.scrollTop - element.clientHeight < 160) {
      void app.loadMoreMessages();
    }
  }

  function openContextMenu(event: MouseEvent, message: ThreadMessage) {
    event.preventDefault();
    contextMenu = { x: event.clientX, y: event.clientY, message };
  }

  function runContextAction(action: MailContextAction) {
    const message = contextMenu?.message;
    if (!message) return;
    switch (action.type) {
      case "reply":
      case "reply-all":
      case "forward":
        void app.composeMessage(message.id, action.type);
        break;
      case "mark-read":
        void app.markMessages([message.id], true);
        break;
      case "mark-unread":
        void app.markMessages([message.id], false);
        break;
      case "move":
        void app.moveMessageToFolder(message.id, action.folderId);
        break;
      case "categorize":
        void app.categorizeMessage(message.id, action.tabId);
        break;
      case "archive":
        void app.archiveThread(message.account_id, message.thread_key);
        break;
      case "delete":
        void app.deleteMessageById(message.id);
        break;
    }
  }

  $effect(() => {
    messages;
    if (appearance.messagePreview === "preview") {
      void app.fillVisibleSnippets();
    }
  });

  onMount(() => {
    const clearBulkSelection = (event: Event) => {
      if (selected.size === 0) return;
      event.preventDefault();
      selected = new Set();
      selectionAnchor = null;
    };
    const clearOnOutsidePointer = (event: PointerEvent) => {
      if (selected.size === 0) return;
      const target = event.target;
      if (
        target instanceof Element &&
        target.closest(".row, .bulk-bar, .mail-context-menu")
      ) {
        return;
      }
      selected = new Set();
      selectionAnchor = null;
    };
    window.addEventListener("auramail:clear-mail-selection", clearBulkSelection);
    window.addEventListener("pointerdown", clearOnOutsidePointer, {
      capture: true
    });
    return () => {
      window.removeEventListener("auramail:clear-mail-selection", clearBulkSelection);
      window.removeEventListener("pointerdown", clearOnOutsidePointer, {
        capture: true
      });
    };
  });
</script>

<section
  class="message-list"
  class:compact-preview={appearance.messagePreview === "compact"}
  class:bulk-selecting={selected.size > 0}
  aria-label="Message list"
>
  <header class="list-head">
    {#if draftsOpen}
      <div class="drafts-head">
        <h2>Drafts</h2>
        <span>{drafts.length}</span>
      </div>
    {:else}
      <div class="search-row">
        <label class="search">
          <Search size={14} />
          <input
            aria-label="Search all mail"
            placeholder="Search all mail"
            value={app.search}
            oninput={onSearch}
          />
        </label>
        {#if app.searchActive}
          <button
            class="icon-btn"
            type="button"
            aria-label="Clear search"
            title="Clear search"
            onclick={() => app.setSearch("")}
          >
            <X size={15} />
          </button>
        {/if}
        <button
          class="compose-btn"
          type="button"
          aria-label="Compose message"
          title="Compose"
          onclick={() => app.openCompose("new")}
        >
          <PenLine size={15} />
        </button>
      </div>

      {#if !app.searchActive}
        <ClassificationTabs />
      {/if}
    {/if}
  </header>

  {#if app.error || app.syncState === "offline" || app.syncState === "error" || app.syncState === "reauth"}
    <div
      class="status-banner"
      class:error={Boolean(app.error) || app.syncState === "error" || app.syncState === "reauth"}
      role={app.syncState === "offline" ? "status" : "alert"}
    >
      <CircleAlert size={14} />
      {#if app.syncState === "reauth"}
        <span>
          {app.reauthAccount?.oauth_provider === "microsoft" ? "Microsoft" : "Google"}
          authorization expired for {app.reauthAccount?.email ?? "an account"}.
        </span>
        <button
          type="button"
          disabled={!app.reauthAccount}
          onclick={() => {
            const account = app.reauthAccount;
            if (account) app.openAccountSettings(account.id);
          }}
        >Open account settings</button>
      {:else}
        <span>
          {#if app.error}
            {app.error}
          {:else if app.syncState === "offline"}
            You're offline. Showing cached mail.
          {:else}
            Sync failed. Showing cached mail.
          {/if}
        </span>
      {/if}
    </div>
  {/if}

  {#if !draftsOpen}
    <div class="bulk-bar">
      <div class="bulk-select-all">
        <Checkbox
          checked={allSelected}
          indeterminate={selected.size > 0 && !allSelected}
          disabled={messages.length === 0}
          label={selected.size > 0 ? `${selected.size} selected` : "Select all"}
          ariaLabel="Select all messages"
          onchange={() => toggleAll()}
        />
      </div>
      <button
        type="button"
        disabled={bulkBusy || selected.size === 0}
        onclick={() => void runBulk((ids) => app.markMessages(ids, true))}
      >
        Mark read
      </button>
      <button
        type="button"
        disabled={bulkBusy || selected.size === 0}
        onclick={() => void runBulk((ids) => app.markMessages(ids, false))}
      >
        Mark unread
      </button>
      <button
        type="button"
        disabled={bulkBusy || app.classifyBusy || selected.size === 0 || !app.aiConfigured}
        title={app.aiConfigured ? "Classify selected" : "Configure Aura AI first"}
        onclick={() => void runBulk((ids) => app.classifyMessagesByIds(ids))}
      >
        <Tags size={13} />
        Classify
      </button>
      <button
        type="button"
        disabled={bulkBusy || selected.size === 0}
        onclick={() => void runBulk((ids) => app.archiveMessages(ids))}
      >
        <Archive size={13} />
        Archive
      </button>
      <button
        class="danger"
        type="button"
        disabled={bulkBusy || selected.size === 0}
        onclick={() => {
          const permanent = [...selected].some((id) => {
            const message = messages.find((candidate) => candidate.id === id);
            return (
              message &&
              app.folders.find((folder) => folder.id === message.folder_id)?.role === "trash"
            );
          });
          if (permanent && !window.confirm("Permanently delete messages already in Trash?")) {
            return;
          }
          void runBulk((ids) => app.deleteMessages(ids));
        }}
      >
        <Trash2 size={13} />
        Delete
      </button>
    </div>
  {/if}

  <div class="rows" role="list" onscroll={onRowsScroll}>
    {#if draftsOpen}
      {#each drafts as draft (draft.id)}
        <div class="row draft">
          <button
            class="draft-open"
            type="button"
            aria-label={`Open draft: ${draftSubject(draft)}`}
            onclick={() => void app.openDraft(draft.id)}
          >
            <span class="draft-icon" aria-hidden="true"><FileText size={13} /></span>
            <span class="row-main">
              <span class="row-top">
                <span class="sender">{draftRecipients(draft)}</span>
                <span class="time">{relativeDate(draft.updatedAt, undefined, appearance.timeFormat)}</span>
              </span>
              <span class="subject">{draftSubject(draft)}</span>
              <span class="snippet">{draftSnippet(draft)}</span>
            </span>
            {#if draft.hasAttachments}
              <span class="row-meta">
                <span class="meta-icon" title="Has attachment"><Paperclip size={13} /></span>
              </span>
            {/if}
          </button>
          <button
            class="draft-delete"
            type="button"
            aria-label={`Delete draft: ${draftSubject(draft)}`}
            title="Delete draft"
            onclick={() => void app.discardDraft(draft.id)}
          >
            <Trash2 size={14} />
          </button>
        </div>
      {:else}
        <div class="empty">
          {#if app.loading}
            Loading drafts…
          {:else}
            No drafts yet.
          {/if}
        </div>
      {/each}
    {:else}
      {#each messages as message (message.id)}
      {@const account = accountFor(message)}
      {@const folderRole = app.folders.find((folder) => folder.id === message.folder_id)?.role}
      {@const inTrash = folderRole === "trash"}
      {@const inArchive =
        !message.thread_has_inbox &&
        (folderRole === "archive" || folderRole === "all")}
      <div
        class="row"
        role="listitem"
        data-message-id={message.id}
        class:unread={!message.is_seen}
        class:selected={app.selectedMessageId === message.id}
        class:checked={selected.has(message.id)}
        class:classifying={app.classifyingMessageIds.has(message.id)}
        class:classified={app.flashingMessageIds.has(message.id)}
        aria-busy={app.classifyingMessageIds.has(message.id)}
        style:--account-color={account ? accountColorValue(account.color, appearance.colorVision, appearance.themeDark) : "var(--accent)"}
        style:--classification-color={classificationColor(message)}
        oncontextmenu={(event) => openContextMenu(event, message)}
      >
        <button
          type="button"
          class="row-open"
          aria-label={`${sender(message)}: ${subject(message)}`}
          aria-pressed={selected.has(message.id)}
          onclick={(event) => onRowClick(event, message.id)}
        >
          <span class="unread-dot" aria-hidden="true"></span>
          <span class="row-main">
            <span class="row-top">
              <span class="sender">{sender(message)}</span>
              {#if message.thread_count > 1}
                <span class="thread-count" title={`${message.thread_count} messages in this thread`}>
                  {message.thread_count}
                </span>
              {/if}
              <span class="time">{relativeDate(message.date, undefined, appearance.timeFormat)}</span>
            </span>
            <span class="subject">{subject(message)}</span>
            <span class="snippet">{message.snippet}</span>
          </span>
          <span class="row-meta">
            {#if showClassificationIcon}
              {@const classificationTab = classificationFor(message)}
              {#if classificationTab}
                {@const ClassificationIcon = tabIconComponent(classificationTab)}
                {@const classificationIndex = app.tabs.findIndex(
                  (tab) => tab.id === classificationTab.id
                )}
                <span
                  class="meta-icon classification-icon"
                  style:color={tabColor(classificationTab, classificationIndex, appearance.colorVision, appearance.themeDark)}
                  title={classificationTab.name}
                >
                  <ClassificationIcon size={13} />
                </span>
              {/if}
            {/if}
            {#if message.has_attachments}
              <span class="meta-icon" title="Has attachment"><Paperclip size={13} /></span>
            {/if}
            {#if message.is_starred}
              <span class="meta-icon starred" title="Starred"><Star size={13} fill="currentColor" /></span>
            {/if}
          </span>
        </button>
        <span class="row-actions">
          <button
            class="row-action"
            type="button"
            aria-label="Reply"
            title="Reply"
            onclick={() => void app.replyToMessage(message.id)}
          >
            <Reply size={13} />
          </button>
          <button
            class="row-action"
            type="button"
            aria-label={message.tab_id === null ? "Classify" : "Reclassify"}
            title={
              !app.aiConfigured
                ? "Configure Aura AI first"
                : message.tab_id === null
                  ? "Classify"
                  : "Reclassify"
            }
            disabled={!app.aiConfigured || app.classifyBusy}
            onclick={() => void app.classifyMessageById(message.id)}
          >
            <Tags size={13} />
          </button>
          {#if !inArchive && message.thread_has_inbox}
            <button
              class="row-action"
              type="button"
              aria-label="Archive"
              title="Archive"
              onclick={() => void app.archiveThread(message.account_id, message.thread_key)}
            >
              <Archive size={13} />
            </button>
          {/if}
          {#if !inTrash}
            <button
              class="row-action danger"
              type="button"
              aria-label="Move to trash"
              title="Move to trash"
              onclick={() => void app.deleteMessageById(message.id)}
            >
              <Trash2 size={13} />
            </button>
          {/if}
        </span>
      </div>
    {:else}
      <div class="empty">
        {#if app.loading}
          Searching…
        {:else if app.searchActive}
          No results for “{app.search.trim()}”.
        {:else}
          No messages yet.
        {/if}
      </div>
      {/each}
    {/if}
  </div>

  {#if contextMenu}
    <MailContextMenu
      x={contextMenu.x}
      y={contextMenu.y}
      message={contextMenu.message}
      folders={app.folders}
      tabs={app.tabs}
      onAction={runContextAction}
      onClose={() => (contextMenu = null)}
    />
  {/if}

  {#if classificationProgress || app.classificationNotice}
    <ClassificationToast
      message={classificationMessage}
      progress={classificationProgress}
      action={app.classificationAction}
      onAbort={() => app.abortClassification()}
      onReclassify={() => void app.classifyVisible()}
      onDismiss={() => app.setClassificationNotice(null)}
    />
  {/if}
</section>

<style>
  .message-list {
    position: relative;
    display: flex;
    flex-direction: column;
    min-width: 0;
    height: 100%;
    overflow: hidden;
    background: var(--bg);
  }
  .list-head {
    flex: none;
    padding: 10px 12px 8px;
    background: var(--bg-raised);
    border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .drafts-head {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
  }
  .drafts-head h2 {
    flex: 1;
    margin: 0;
    color: var(--text-strong);
    font-size: 13px;
    font-weight: 650;
  }
  .drafts-head span {
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--bg-hover);
    color: var(--muted);
    font-size: 10.5px;
    font-weight: 650;
  }
  .search-row {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 7px;
    flex: 1;
    min-width: 0;
    height: 30px;
    padding: 0 9px;
    border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
    border-radius: 8px;
    background: var(--bg);
    color: var(--muted);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .search input {
    width: 100%;
    min-width: 0;
    border: 0;
    outline: 0;
    background: none;
  }
  .search input::placeholder {
    color: var(--muted);
  }
  .compose-btn {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    flex: none;
    border: 1px solid color-mix(in srgb, var(--accent) 28%, var(--border));
    border-radius: 8px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    color: var(--accent);
  }
  .compose-btn:hover {
    background: color-mix(in srgb, var(--accent) 17%, transparent);
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
  }
  .icon-btn {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    flex: none;
    border-radius: 8px;
    color: var(--muted);
  }
  .bulk-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    flex: none;
    padding: 7px 10px;
    border-bottom: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    background: color-mix(in srgb, var(--accent) 9%, var(--bg));
    font-size: 11px;
  }
  .bulk-select-all {
    display: flex;
    align-items: center;
    margin-right: auto;
  }
  .bulk-bar button {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 24px;
    padding: 0 7px;
    border-radius: 6px;
    color: var(--muted);
    font-size: 10.5px;
    font-weight: 600;
  }
  .bulk-bar button:hover:not(:disabled) {
    color: var(--text);
    background: var(--bg-hover);
  }
  .bulk-bar button.danger:hover:not(:disabled) {
    color: var(--danger);
  }
  .bulk-bar button:disabled {
    cursor: default;
    opacity: 0.5;
  }
  .icon-btn:hover:not(:disabled) {
    color: var(--text);
    background: var(--bg-hover);
  }
  .icon-btn:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .status-banner {
    display: flex;
    align-items: center;
    gap: 7px;
    flex: none;
    padding: 7px 12px;
    border-bottom: 1px solid color-mix(in srgb, var(--warning) 30%, transparent);
    background: color-mix(in srgb, var(--warning) 10%, transparent);
    color: var(--text);
    font-size: 11px;
    line-height: 1.4;
  }
  .status-banner :global(svg) {
    flex: none;
    color: var(--warning);
  }
  .status-banner.error {
    border-bottom-color: color-mix(in srgb, var(--danger) 30%, transparent);
    background: color-mix(in srgb, var(--danger) 10%, transparent);
  }
  .status-banner.error :global(svg) {
    color: var(--danger);
  }
  .status-banner span {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .status-banner button {
    flex: none;
    padding: 2px 8px;
    border: 1px solid color-mix(in srgb, var(--danger) 40%, transparent);
    border-radius: 6px;
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    color: var(--danger);
    font-size: 10.5px;
    font-weight: 600;
    white-space: nowrap;
  }
  .status-banner button:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .rows {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    contain: layout paint;
    overscroll-behavior: contain;
  }
  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 0;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 38%, transparent);
    text-align: left;
    content-visibility: auto;
    contain-intrinsic-block-size: auto 88px;
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row.selected {
    background: color-mix(in srgb, var(--text) 5%, var(--bg-raised));
  }
  .row.checked {
    background: color-mix(in srgb, var(--accent) 15%, var(--bg-raised));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .row-open {
    display: flex;
    gap: 8px;
    flex: 1;
    min-width: 0;
    padding: 9px 8px 9px 10px;
    outline: 0;
    text-align: left;
  }
  .row-open:focus-visible {
    outline: 0;
    box-shadow: none;
  }
  .row-actions {
    position: absolute;
    top: 50%;
    right: 30px;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 1px;
    padding: 2px;
    border-radius: 8px;
    background: var(--bg-hover);
    box-shadow: -12px 0 14px color-mix(in srgb, var(--bg-hover) 82%, transparent);
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    transform: translateY(-50%);
  }
  .row:hover .row-actions {
    opacity: 1;
    visibility: visible;
    pointer-events: auto;
  }
  .bulk-selecting .row-actions {
    display: none;
  }
  .row-action {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    color: var(--muted);
  }
  .row-action:hover:not(:disabled) {
    color: var(--text);
    background: var(--bg-hover);
  }
  .row-action.danger:hover:not(:disabled) {
    color: var(--danger);
  }
  .row-action:disabled {
    cursor: default;
    opacity: 0.4;
  }
  .row::before {
    content: "";
    position: absolute;
    top: 5px;
    bottom: 5px;
    left: 3px;
    width: 3px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--account-color, var(--accent)) 58%, transparent);
    pointer-events: none;
  }
  .row.selected::before {
    background: var(--account-color, var(--accent));
  }
  .row.draft {
    padding: 0;
    gap: 0;
  }
  .row.draft::before {
    display: none;
  }
  .draft-open {
    display: flex;
    gap: 8px;
    flex: 1;
    min-width: 0;
    padding: 10px 4px 10px 12px;
    text-align: left;
  }
  .draft-icon {
    display: grid;
    place-items: center;
    width: 16px;
    height: 18px;
    margin-top: 2px;
    flex: none;
    color: var(--accent);
  }
  .draft-delete {
    display: grid;
    place-items: center;
    width: 34px;
    flex: none;
    color: var(--muted);
  }
  .draft-delete:hover {
    color: var(--danger, #e5484d);
    background: var(--bg-hover);
  }
  .unread-dot {
    width: 6px;
    height: 6px;
    margin-top: 7px;
    flex: none;
    border-radius: 50%;
    background: transparent;
  }
  .row.unread .unread-dot {
    background: var(--accent);
  }
  .row-main {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .row-top {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .sender {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--text);
    font-size: 12.5px;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row.unread .sender,
  .row.unread .subject {
    color: var(--text-strong);
    font-weight: 650;
  }
  .thread-count {
    display: grid;
    place-items: center;
    min-width: 17px;
    height: 17px;
    flex: none;
    padding: 0 4px;
    border-radius: 999px;
    background: var(--bg-hover);
    color: var(--muted);
    font-size: 9.5px;
    font-weight: 650;
  }
  .time {
    display: inline-flex;
    align-items: center;
    flex: none;
    color: var(--muted);
    font-size: 10.5px;
  }
  .row.classifying {
    animation: classify-pulse 1.3s ease-in-out infinite;
  }
  @keyframes classify-pulse {
    0%, 100% {
      background: transparent;
    }
    50% {
      background: color-mix(in srgb, var(--accent) 9%, transparent);
    }
  }
  .row.classified::after {
    content: "";
    position: absolute;
    inset: 0;
    z-index: 0;
    pointer-events: none;
    background: var(--classification-color, var(--accent));
    opacity: 0;
    animation: classification-flash 1.05s ease-in-out;
  }
  @keyframes classification-flash {
    0%, 100% {
      opacity: 0;
    }
    32%, 66% {
      opacity: 0.14;
    }
  }
  .subject {
    overflow: hidden;
    color: var(--text);
    font-size: 12.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .snippet {
    display: -webkit-box;
    overflow: hidden;
    color: var(--muted);
    font-size: 11.5px;
    line-height: 1.35;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }
  .compact-preview .snippet {
    display: none;
  }
  .compact-preview .row {
    contain-intrinsic-block-size: auto 56px;
  }
  .row-meta {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    flex: none;
    padding-top: 2px;
    color: var(--muted);
  }
  .starred {
    color: var(--warning);
  }
  .empty {
    padding: 28px 16px;
    color: var(--muted);
    font-size: 12px;
    text-align: center;
  }
</style>
