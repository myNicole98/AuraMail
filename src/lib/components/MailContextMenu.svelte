<script lang="ts">
  import { onMount } from "svelte";
  import {
    Archive,
    ChevronRight,
    FolderInput,
    Mail,
    MailOpen,
    Reply,
    ReplyAll,
    Send,
    Tags,
    Trash2
  } from "lucide-svelte";
  import { tabColor, tabIconComponent } from "../tabAppearance";
  import { appearance } from "../stores/appearance.svelte";
  import type { Folder, Tab, ThreadMessage } from "../types";

  export type MailContextAction =
    | {
        type:
          | "reply"
          | "reply-all"
          | "forward"
          | "mark-read"
          | "mark-unread"
          | "archive"
          | "delete";
      }
    | { type: "move"; folderId: number }
    | { type: "categorize"; tabId: number | null };

  let {
    x,
    y,
    message,
    folders,
    tabs,
    onAction,
    onClose
  }: {
    x: number;
    y: number;
    message: ThreadMessage;
    folders: Folder[];
    tabs: Tab[];
    onAction: (action: MailContextAction) => void;
    onClose: () => void;
  } = $props();

  let menu = $state<HTMLElement | null>(null);
  let submenu = $state<"move" | "categorize" | null>(null);
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  let closed = false;
  let movableFolders = $derived(
    folders.filter(
      (folder) =>
        folder.account_id === message.account_id &&
        folder.id !== message.folder_id &&
        folder.role !== "drafts" &&
        folder.role !== "all"
    )
  );
  let inTrash = $derived(
    folders.find((folder) => folder.id === message.folder_id)?.role === "trash"
  );
  let left = $derived(Math.max(8, Math.min(x, window.innerWidth - 232)));
  let top = $derived(Math.max(8, Math.min(y, window.innerHeight - 336)));

  function close() {
    if (closed) return;
    closed = true;
    onClose();
  }

  function choose(action: MailContextAction) {
    onAction(action);
    submenu = null;
    close();
  }

  function cancelSubmenuHover() {
    clearTimeout(hoverTimer);
    hoverTimer = undefined;
  }

  function hoverSubmenu(name: "move" | "categorize") {
    cancelSubmenuHover();
    hoverTimer = setTimeout(() => {
      submenu = name;
      hoverTimer = undefined;
    }, 140);
  }

  function toggleSubmenu(name: "move" | "categorize") {
    cancelSubmenuHover();
    submenu = submenu === name ? null : name;
  }

  onMount(() => {
    const outside = (event: PointerEvent) => {
      if (!(event.target instanceof Node)) return;
      if (menu?.contains(event.target)) return;
      close();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      close();
    };
    window.addEventListener("pointerdown", outside);
    window.addEventListener("keydown", escape, { capture: true });
    menu?.querySelector<HTMLButtonElement>("button")?.focus();
    return () => {
      cancelSubmenuHover();
      window.removeEventListener("pointerdown", outside);
      window.removeEventListener("keydown", escape, { capture: true });
    };
  });
</script>

<button
  type="button"
  class="menu-backdrop"
  aria-label="Close message menu"
  onclick={close}
></button>

<div
  bind:this={menu}
  class="mail-context-menu"
  style:left={`${left}px`}
  style:top={`${top}px`}
  role="menu"
  aria-label="Message actions"
>
  <button type="button" role="menuitem" onclick={() => choose({ type: "reply" })}>
    <Reply size={14} />
    Reply
  </button>
  <button type="button" role="menuitem" onclick={() => choose({ type: "reply-all" })}>
    <ReplyAll size={14} />
    Reply all
  </button>
  <button type="button" role="menuitem" onclick={() => choose({ type: "forward" })}>
    <Send size={14} />
    Forward
  </button>
  <div class="divider" role="separator"></div>
  <button
    type="button"
    role="menuitem"
    onclick={() =>
      choose({ type: message.is_seen ? "mark-unread" : "mark-read" })}
  >
    {#if message.is_seen}
      <Mail size={14} />
      Mark unread
    {:else}
      <MailOpen size={14} />
      Mark read
    {/if}
  </button>
  <button
    type="button"
    role="menuitem"
    aria-haspopup="menu"
    aria-expanded={submenu === "move"}
    onclick={() => toggleSubmenu("move")}
    onpointerenter={() => hoverSubmenu("move")}
    onpointerleave={cancelSubmenuHover}
  >
    <FolderInput size={14} />
    Move to folder
    <span class="chevron"><ChevronRight size={13} /></span>
  </button>
  <button
    type="button"
    role="menuitem"
    aria-haspopup="menu"
    aria-expanded={submenu === "categorize"}
    onclick={() => toggleSubmenu("categorize")}
    onpointerenter={() => hoverSubmenu("categorize")}
    onpointerleave={cancelSubmenuHover}
  >
    <Tags size={14} />
    Categorize
    <span class="chevron"><ChevronRight size={13} /></span>
  </button>
  {#if submenu === "move"}
    <div class="submenu move" role="menu" aria-label="Move to folder">
      {#each movableFolders as folder (folder.id)}
        <button
          type="button"
          role="menuitem"
          onclick={() => choose({ type: "move", folderId: folder.id })}
        >
          <FolderInput size={13} />
          {folder.name}
        </button>
      {:else}
        <span class="empty">No other folders</span>
      {/each}
    </div>
  {:else if submenu === "categorize"}
    <div class="submenu categorize" role="menu" aria-label="Categorize">
      <button
        type="button"
        role="menuitem"
        class:active={message.tab_id === null}
        onclick={() => choose({ type: "categorize", tabId: null })}
      >
        Uncategorized
      </button>
      {#each tabs as tab, index (tab.id)}
        {@const TabIcon = tabIconComponent(tab)}
        {@const color = tabColor(tab, index, appearance.colorVision, appearance.themeDark)}
        <button
          type="button"
          role="menuitem"
          class:active={message.tab_id === tab.id}
          onclick={() => choose({ type: "categorize", tabId: tab.id })}
        >
          <span class="tab-badge" style:color={color} data-tab-color={color}>
            <TabIcon size={12} />
          </span>
          {tab.name}
        </button>
      {/each}
    </div>
  {/if}
  <div class="divider" role="separator"></div>
  {#if message.thread_has_inbox}
    <button type="button" role="menuitem" onclick={() => choose({ type: "archive" })}>
      <Archive size={14} />
      Archive
    </button>
  {/if}
  <button
    type="button"
    role="menuitem"
    class="danger"
    onclick={() => choose({ type: "delete" })}
  >
    <Trash2 size={14} />
    {inTrash ? "Delete permanently" : "Delete"}
  </button>
</div>

<style>
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 79;
    border: 0;
    padding: 0;
    background: transparent;
    cursor: default;
  }
  .mail-context-menu {
    position: fixed;
    z-index: 80;
    display: flex;
    flex-direction: column;
    width: 224px;
    padding: 5px;
    border: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
    border-radius: 10px;
    background: var(--bg-raised);
    box-shadow: 0 12px 32px color-mix(in srgb, #000 26%, transparent);
    color: var(--text);
    font-size: 12px;
  }
  .mail-context-menu > button,
  .submenu button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 28px;
    padding: 0 8px;
    border-radius: 6px;
    color: var(--text);
    text-align: left;
  }
  .mail-context-menu > button:hover,
  .mail-context-menu > button:focus-visible,
  .submenu button:hover,
  .submenu button:focus-visible {
    background: var(--bg-hover);
    outline: 0;
  }
  .mail-context-menu button.danger {
    color: var(--danger);
  }
  .mail-context-menu button.danger:hover {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
  }
  .divider {
    height: 1px;
    margin: 4px 6px;
    background: color-mix(in srgb, var(--border) 55%, transparent);
  }
  .chevron {
    margin-left: auto;
    color: var(--muted);
  }
  .submenu {
    position: absolute;
    left: calc(100% + 4px);
    top: 64px;
    max-height: 260px;
    width: 190px;
    overflow-y: auto;
    padding: 5px;
    border: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
    border-radius: 9px;
    background: var(--bg-raised);
    box-shadow: 0 10px 26px color-mix(in srgb, #000 22%, transparent);
  }
  .submenu.categorize {
    top: 96px;
  }
  .submenu button.active {
    color: var(--accent);
  }
  .tab-badge {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    flex: none;
    border-radius: 5px;
    background: color-mix(in srgb, currentColor 14%, transparent);
  }
  .empty {
    display: block;
    padding: 8px;
    color: var(--muted);
  }
</style>
