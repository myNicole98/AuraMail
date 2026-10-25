<script lang="ts">
  import { onMount } from "svelte";
  import {
    Archive,
    ChevronDown,
    FileText,
    Forward,
    Inbox,
    MoreHorizontal,
    Paperclip,
    Reply,
    Search,
    Send,
    Settings,
    Star,
    Trash2,
    User
  } from "lucide-svelte";
  import type { ThemeBackground, ThemePanes } from "../../settings";
  import { appearance } from "../../stores/appearance.svelte";

  let { label }: { label: string } = $props();

  const messages = [
    {
      sender: "Ada Lovelace",
      subject: "Project update",
      snippet: "The latest notes are ready for review.",
      time: "10:24",
      unread: true,
      selected: true,
      attachment: true
    },
    {
      sender: "Grace Hopper",
      subject: "Compiler notes",
      snippet: "I found a small issue in the parser.",
      time: "09:48",
      unread: false,
      selected: false,
      attachment: false
    },
    {
      sender: "Alan Turing",
      subject: "Lunch?",
      snippet: "Are we still on for today?",
      time: "Yesterday",
      unread: false,
      selected: false,
      attachment: false
    },
    {
      sender: "Katherine Johnson",
      subject: "Orbit calculations",
      snippet: "The numbers check out.",
      time: "Mon",
      unread: false,
      selected: false,
      attachment: false
    }
  ];

  let background = $state<ThemeBackground>(
    (document.documentElement.dataset.themeBackground as ThemeBackground) ??
      "solid"
  );
  let panes = $state<ThemePanes>(
    (document.documentElement.dataset.themePanes as ThemePanes) ?? "straight"
  );

  onMount(() => {
    const sync = () => {
      background =
        (document.documentElement.dataset.themeBackground as ThemeBackground) ??
        "solid";
      panes =
        (document.documentElement.dataset.themePanes as ThemePanes) ?? "straight";
    };
    sync();
    window.addEventListener("auramail:theme-applied", sync);
    return () => window.removeEventListener("auramail:theme-applied", sync);
  });
</script>

<div class="preview" aria-hidden="true">
  <header>
    <img class="logo" src="/aura_mail_mark.svg" alt="" />
    <span class="brand">AuraMail</span>
    <span class="label">{label}</span>
  </header>
  <div
    class="mail"
    class:glass={background === "glass"}
    class:rounded={panes === "rounded"}
    class:focus={appearance.layoutMode === "reader"}
    class:stacked={appearance.layoutMode === "stacked"}
  >
    <aside>
      <div class="space-select">
        <span class="space-icon"><User size={11} /></span>
        <span class="space-name">Personal</span>
        <ChevronDown size={10} />
      </div>
      <div class="nav-item active">
        <Inbox size={12} />
        <span>Inbox</span>
        <span class="count">12</span>
      </div>
      <div class="nav-item">
        <Star size={12} />
        <span>Starred</span>
      </div>
      <div class="nav-item">
        <Send size={12} />
        <span>Sent</span>
      </div>
      <div class="nav-item">
        <FileText size={12} />
        <span>Drafts</span>
      </div>
      <div class="nav-item">
        <Archive size={12} />
        <span>Archive</span>
      </div>
      <div class="nav-item">
        <Trash2 size={12} />
        <span>Trash</span>
      </div>

      <div class="account-group">
        <div class="account-head">
          <span class="account-dot"></span>
          <span>you@auramail.app</span>
          <ChevronDown size={10} />
        </div>
        <div class="folder-row"><Inbox size={10} /><span>Inbox</span></div>
        <div class="folder-row"><FileText size={10} /><span>Drafts</span></div>
      </div>

      <div class="sidebar-foot">
        <span class="sync-dot"></span>
        <span>Synced</span>
        <Settings size={11} />
      </div>
    </aside>

    <div class="list">
      <div class="list-head">
        <div class="search">
          <Search size={10} />
          <span>Search mail</span>
        </div>
      </div>
      {#each messages as message (message.sender)}
        <div
          class="row"
          class:selected={message.selected}
          class:unread={message.unread}
        >
          <span class="unread-dot"></span>
          <div class="row-main">
            <div class="row-top">
              <span class="sender">{message.sender}</span>
              <span class="time">{message.time}</span>
            </div>
            <span class="subject">{message.subject}</span>
            <span class="snippet">{message.snippet}</span>
          </div>
          {#if message.attachment}
            <span class="row-meta"><Paperclip size={10} /></span>
          {/if}
        </div>
      {/each}
    </div>

    <article>
      <div class="reader-head">
        <span class="avatar">AL</span>
        <div class="reader-sender">
          <strong>Ada Lovelace</strong>
          <small>to me</small>
        </div>
        <span class="reader-time">10:24</span>
        <span class="reader-actions">
          <Reply size={11} />
          <Forward size={11} />
          <MoreHorizontal size={11} />
        </span>
      </div>
      <h2>Quietly better mail.</h2>
      <p>
        The latest notes are ready for review. I gathered the open questions and
        the decisions we made last week.
      </p>
      <p>Let me know if anything needs another pass before tomorrow.</p>
      <div class="attachment">
        <Paperclip size={10} />
        <span>notes.pdf</span>
      </div>
    </article>
  </div>
</div>

<style>
  .preview {
    display: flex;
    flex-direction: column;
    gap: 14px;
    height: 100%;
    padding: 14px;
    border: 1px solid color-mix(in srgb, var(--text) 12%, transparent);
    border-radius: 18px;
    background: color-mix(in srgb, var(--bg-raised) 24%, transparent);
    box-shadow:
      0 24px 70px color-mix(in srgb, var(--bg) 48%, transparent),
      inset 0 1px 0 color-mix(in srgb, var(--text) 8%, transparent);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }
  .logo {
    width: 36px;
    height: 36px;
    flex: none;
    border-radius: 10px;
    background: color-mix(in srgb, var(--accent) 9%, transparent);
    box-shadow: 0 0 18px color-mix(in srgb, var(--accent) 24%, transparent);
  }
  .brand {
    color: var(--text-strong);
    font-size: 12px;
    font-weight: 680;
  }
  .label {
    margin-left: auto;
    color: var(--muted);
    font-size: 8px;
    letter-spacing: 0.13em;
    text-transform: uppercase;
  }
  .mail {
    display: grid;
    grid-template-columns: 24% 36% minmax(0, 1fr);
    flex: 1;
    min-height: 0;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
    border-radius: 11px;
    background: var(--bg);
  }
  aside,
  .list,
  article {
    padding: 9px 8px;
  }
  aside,
  .list {
    border-right: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  aside {
    display: flex;
    flex-direction: column;
    min-width: 0;
    color: var(--muted);
  }
  .space-select,
  .account-head,
  .nav-item,
  .folder-row,
  .sidebar-foot,
  .reader-head {
    display: flex;
    align-items: center;
  }
  .space-select {
    gap: 5px;
    margin-bottom: 10px;
    color: var(--text);
    font-size: 8px;
    font-weight: 650;
  }
  .space-icon {
    display: grid;
    place-items: center;
    width: 17px;
    height: 17px;
    flex: none;
    border-radius: 5px;
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--accent);
  }
  .space-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .space-select :global(svg:last-child) {
    margin-left: auto;
    flex: none;
  }
  .nav-item {
    gap: 6px;
    min-height: 23px;
    padding: 0 5px;
    border-radius: 5px;
    font-size: 8px;
  }
  .nav-item :global(svg) {
    flex: none;
  }
  .nav-item span:nth-child(2) {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .nav-item.active {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--text-strong);
  }
  .nav-item.active :global(svg) {
    color: var(--accent);
  }
  .count {
    color: var(--accent);
    font-size: 7px;
    font-weight: 700;
  }
  .account-group {
    margin-top: 11px;
    padding-top: 8px;
    border-top: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .account-head {
    gap: 5px;
    margin-bottom: 4px;
    padding: 0 5px;
    color: var(--text);
    font-size: 7.5px;
  }
  .account-head span:nth-child(2) {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .account-dot,
  .sync-dot {
    width: 5px;
    height: 5px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
  }
  .folder-row {
    gap: 6px;
    min-height: 20px;
    padding: 0 9px;
    font-size: 7.5px;
  }
  .sidebar-foot {
    gap: 5px;
    margin-top: auto;
    padding: 7px 5px 0;
    border-top: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    color: var(--muted);
    font-size: 7px;
  }
  .sidebar-foot :global(svg) {
    margin-left: auto;
  }
  .sync-dot {
    background: var(--success);
  }
  .list {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .list-head {
    flex: none;
    padding-bottom: 7px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 5px;
    min-height: 24px;
    padding: 0 7px;
    border: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    border-radius: 6px;
    background: color-mix(in srgb, var(--bg) 40%, transparent);
    color: var(--muted);
    font-size: 7.5px;
  }
  .search :global(svg) {
    flex: none;
  }
  .row {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    min-height: 48px;
    padding: 7px 6px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 35%, transparent);
  }
  .row.selected {
    border-radius: 6px;
    border-bottom-color: transparent;
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    outline: 1px solid color-mix(in srgb, var(--accent) 28%, transparent);
  }
  .unread-dot {
    width: 5px;
    height: 5px;
    margin-top: 4px;
    flex: none;
    border-radius: 50%;
    background: transparent;
  }
  .row.unread .unread-dot {
    background: var(--accent);
  }
  .row-main {
    min-width: 0;
    flex: 1;
  }
  .row-top {
    display: flex;
    align-items: baseline;
    gap: 5px;
  }
  .sender {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--text);
    font-size: 8px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row.unread .sender {
    color: var(--text-strong);
    font-weight: 700;
  }
  .time {
    flex: none;
    color: var(--muted);
    font-size: 6.5px;
  }
  .subject {
    display: block;
    margin-top: 2px;
    overflow: hidden;
    color: var(--text-strong);
    font-size: 8px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .snippet {
    display: block;
    margin-top: 2px;
    overflow: hidden;
    color: var(--muted);
    font-size: 7px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-meta {
    flex: none;
    margin-top: 3px;
    color: var(--muted);
  }
  article {
    position: relative;
    min-width: 0;
    overflow: hidden;
  }
  .reader-head {
    gap: 7px;
    padding-bottom: 9px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 45%, transparent);
  }
  .avatar {
    display: grid;
    place-items: center;
    width: 23px;
    height: 23px;
    flex: none;
    border-radius: 50%;
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--accent);
    font-size: 7px;
    font-weight: 700;
  }
  .reader-sender {
    min-width: 0;
    flex: 1;
  }
  .reader-sender strong {
    display: block;
    overflow: hidden;
    color: var(--text-strong);
    font-size: 8.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .reader-sender small {
    display: block;
    margin-top: 1px;
    color: var(--muted);
    font-size: 7px;
  }
  .reader-time {
    flex: none;
    color: var(--muted);
    font-size: 7px;
  }
  .reader-actions {
    display: flex;
    gap: 6px;
    flex: none;
    color: var(--muted);
  }
  h2 {
    margin: 13px 0 9px;
    color: var(--text-strong);
    font-size: 15px;
    line-height: 1.15;
    letter-spacing: -0.03em;
  }
  article p {
    margin: 0 0 8px;
    color: var(--text);
    font-size: 8px;
    line-height: 1.55;
  }
  .attachment {
    display: flex;
    align-items: center;
    gap: 5px;
    width: fit-content;
    margin-top: 14px;
    padding: 5px 7px;
    border: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    border-radius: 5px;
    color: var(--muted);
    font-size: 7px;
  }
  .mail.glass {
    border-color: color-mix(in srgb, var(--text) 12%, transparent);
    background: transparent;
  }
  .mail.glass aside {
    background: color-mix(in srgb, var(--bg-raised) 42%, transparent);
  }
  .mail.glass .list {
    background: color-mix(in srgb, var(--bg) 32%, transparent);
  }
  .mail.glass article {
    background: color-mix(in srgb, var(--bg) 22%, transparent);
  }
  .mail.rounded {
    gap: 3px;
    padding: 3px;
    border: 0;
    background: transparent;
  }
  .mail.rounded aside,
  .mail.rounded .list,
  .mail.rounded article {
    border: 1px solid color-mix(in srgb, var(--text) 12%, transparent);
    border-radius: 6px;
  }
  .mail.rounded aside,
  .mail.rounded .list {
    border-right: 1px solid color-mix(in srgb, var(--text) 12%, transparent);
  }
  .focus {
    grid-template-columns: 24% 0 minmax(0, 1fr);
  }
  .focus .list {
    overflow: hidden;
    opacity: 0;
    padding: 0;
    border: 0;
  }
  .stacked {
    grid-template-columns: 24% minmax(0, 1fr);
    grid-template-rows: 42% 58%;
  }
  .stacked aside {
    grid-row: 1 / 3;
  }
  .stacked .list {
    border-right: 0;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  @media (max-width: 899px) {
    .preview {
      padding: 9px;
      gap: 9px;
    }
    .brand {
      display: none;
    }
    .mail {
      grid-template-columns: 22% 34% minmax(0, 1fr);
    }
    h2 {
      font-size: 13px;
    }
    .nav-item,
    .sender,
    .subject {
      font-size: 7px;
    }
    .snippet,
    .list-head {
      display: none;
    }
  }
  @media (max-width: 639px) {
    .preview {
      height: 100%;
      border-radius: 12px;
    }
    .mail {
      grid-template-columns: 22% 78%;
    }
    article {
      display: none;
    }
  }
</style>
