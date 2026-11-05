<script lang="ts">
  import { onMount } from "svelte";
  import {
    Archive,
    ArrowLeft,
    Ban,
    Bell,
    BrainCircuit,
    CalendarDays,
    ChevronDown,
    FileText,
    Inbox,
    Layers,
    Mail,
    Mails,
    Paperclip,
    PanelLeftClose,
    PenLine,
    RefreshCw,
    Search,
    Send,
    Settings,
    Star,
    Trash2
  } from "lucide-svelte";
  import type { ThemeBackground, ThemePanes } from "../../settings";
  import { appearance } from "../../stores/appearance.svelte";

  const messages = [
    {
      sender: "Ada Lovelace",
      initials: "AL",
      subject: "Project update",
      snippet: "The latest notes are ready for review.",
      time: "10:24",
      unread: true,
      selected: true,
      attachment: true
    },
    {
      sender: "Grace Hopper",
      initials: "GH",
      subject: "Compiler notes",
      snippet: "I found a small issue in the parser.",
      time: "09:48",
      unread: false,
      selected: false,
      attachment: false
    },
    {
      sender: "Alan Turing",
      initials: "AT",
      subject: "Lunch?",
      snippet: "Are we still on for today?",
      time: "Yesterday",
      unread: false,
      selected: false,
      attachment: false
    },
    {
      sender: "Katherine Johnson",
      initials: "KJ",
      subject: "Orbit calculations",
      snippet: "The numbers check out.",
      time: "Mon",
      unread: false,
      selected: false,
      attachment: false
    },
    {
      sender: "Margaret Hamilton",
      initials: "MH",
      subject: "Apollo guidance",
      snippet: "The priority display is ready to test.",
      time: "Mon",
      unread: false,
      selected: false,
      attachment: true
    },
    {
      sender: "Barbara Liskov",
      initials: "BL",
      subject: "Substitution notes",
      snippet: "I cleaned up the examples from yesterday.",
      time: "Sun",
      unread: false,
      selected: false,
      attachment: false
    },
    {
      sender: "Edsger Dijkstra",
      initials: "ED",
      subject: "Shortest path",
      snippet: "A shorter route through the graph.",
      time: "Fri",
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
    // The appearance step can change the theme before the event reaches this
    // preview during onboarding; watch the root attributes as well.
    const observer = new MutationObserver(sync);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme-panes", "data-theme-background"]
    });
    return () => {
      window.removeEventListener("auramail:theme-applied", sync);
      observer.disconnect();
    };
  });
</script>

<div class="preview" aria-hidden="true">
  <div
    class="mail"
    class:modern={background === "modern"}
    class:rounded={panes === "rounded"}
    class:focus={appearance.layoutMode === "reader"}
    class:stacked={appearance.layoutMode === "stacked"}
    class:compact-list={appearance.messagePreview === "compact"}
    class:icon-tabs={appearance.iconOnlyTabs}
  >
    <aside class="rail">
      <span class="rail-button active"><Mail size={12} /></span>
      <span class="rail-button"><CalendarDays size={12} /></span>
      <span class="rail-button rail-settings"><Settings size={12} /></span>
    </aside>

    <aside class="sidebar">
      <div class="space-select">
        <span class="space-icon"><Layers size={10} /></span>
        <span class="space-name">Personal</span>
        <ChevronDown size={9} />
      </div>

      <div class="nav-item active">
        <Inbox size={11} />
        <span>Inbox</span>
        <span class="count">2</span>
      </div>
      <div class="nav-item"><Send size={11} /><span>Sent</span></div>
      <div class="nav-item"><Archive size={11} /><span>Archive</span></div>
      <div class="nav-item"><Ban size={11} /><span>Spam</span></div>
      <div class="nav-item"><Trash2 size={11} /><span>Trash</span></div>
      <div class="nav-item"><Star size={11} /><span>Starred</span></div>
      <div class="nav-item"><FileText size={11} /><span>Drafts</span></div>

      <div class="account">
        <div class="account-head">
          <ChevronDown size={9} />
          <span class="account-dot"></span>
          <span class="account-name">you@auramail.app</span>
        </div>
        <div class="folder-row">
          <Inbox size={9} /><span>Inbox</span><span class="count">2</span>
        </div>
        <div class="folder-row"><Send size={9} /><span>Sent</span></div>
      </div>

      <div class="sidebar-foot">
        <span class="sync-dot"></span>
        <span>Synced</span>
        <RefreshCw size={10} />
        <PanelLeftClose size={10} />
      </div>
    </aside>

    <div class="list">
      <div class="list-head">
        <div class="search-row">
          <div class="search"><Search size={9} /><span>Search all mail</span></div>
          <span class="compose"><PenLine size={10} /></span>
        </div>
        <div class="tabs">
          <span class="tab all active"><Mails size={9} />{#if !appearance.iconOnlyTabs}<span>All</span>{/if}</span>
          <span class="tab primary"><Inbox size={9} />{#if !appearance.iconOnlyTabs}<span>Primary</span>{/if}</span>
          <span class="tab updates"><Bell size={9} />{#if !appearance.iconOnlyTabs}<span>Updates</span>{/if}</span>
        </div>
      </div>

      {#each messages as message (message.sender)}
        <div
          class="row"
          class:selected={message.selected}
          class:unread={message.unread}
        >
          {#if !appearance.listAvatars}
            <span class="unread-dot"></span>
          {/if}
          {#if appearance.listAvatars}
            <span class="row-avatar" class:unread={message.unread}>{message.initials}</span>
          {/if}
          <div class="row-main">
            <div class="row-top">
              <span class="sender">{message.sender}</span>
              <span class="time">{message.time}</span>
            </div>
            <span class="subject">{message.subject}</span>
            <span class="snippet">{message.snippet}</span>
          </div>
          {#if message.attachment}
            <span class="row-meta"><Paperclip size={9} /></span>
          {/if}
        </div>
      {/each}
    </div>

    <article>
      <div class="reader-head">
        <div class="subject-row">
          <span class="back"><ArrowLeft size={10} /></span>
          <h1>Project update</h1>
          <span class="aura"><BrainCircuit size={10} /></span>
          <span class="reply">Reply All <ChevronDown size={9} /></span>
        </div>
        <div class="byline">
          <span class="avatar">AL</span>
          <div class="who">
            <strong>Ada Lovelace</strong>
            <span>ada@example.com</span>
            <span>to you · 10:24</span>
          </div>
          <span class="tag">Attachments</span>
        </div>
      </div>

      <div class="reader-body">
        <div class="attachments">
          <Paperclip size={9} />
          <span>notes.pdf</span>
        </div>
        <p>
          The latest notes are ready for review. I gathered the open questions
          and the decisions we made last week.
        </p>
        <p>Let me know if anything needs another pass before tomorrow.</p>
      </div>
    </article>
  </div>
</div>

<style>
  .preview {
    height: 100%;
    padding: 12px;
    border: 1px solid color-mix(in srgb, var(--text) 12%, transparent);
    border-radius: 18px;
    background: color-mix(in srgb, var(--bg-raised) 24%, transparent);
    box-shadow:
      0 24px 70px color-mix(in srgb, var(--bg) 48%, transparent),
      inset 0 1px 0 color-mix(in srgb, var(--text) 8%, transparent);
  }
  .mail {
    display: grid;
    grid-template-columns: 20px 24% 33% minmax(0, 1fr);
    height: 100%;
    min-height: 0;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
    border-radius: 11px;
    background: var(--bg);
  }
  .rail,
  .sidebar {
    background: var(--bg-raised);
  }
  .rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 7px 0;
    border-right: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .rail-button {
    display: grid;
    place-items: center;
    width: 15px;
    height: 15px;
    border-radius: 4px;
    color: var(--muted);
  }
  .rail-button.active {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
  }
  .rail-settings {
    margin-top: auto;
  }
  .sidebar {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: 7px 6px;
    color: var(--muted);
    border-right: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .space-select,
  .account-head,
  .nav-item,
  .folder-row,
  .sidebar-foot,
  .subject-row,
  .byline,
  .tabs,
  .search-row,
  .attachments {
    display: flex;
    align-items: center;
  }
  .space-select {
    gap: 5px;
    margin-bottom: 8px;
    color: var(--text);
    font-size: 7.5px;
    font-weight: 650;
  }
  .space-icon {
    display: grid;
    place-items: center;
    width: 15px;
    height: 15px;
    flex: none;
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--accent);
  }
  .space-name,
  .account-name {
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
    gap: 5px;
    min-height: 20px;
    padding: 0 4px;
    border-radius: 4px;
    font-size: 7.5px;
  }
  .nav-item :global(svg) {
    flex: none;
  }
  .nav-item span:not(.count) {
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
    font-size: 6.5px;
    font-weight: 700;
  }
  .account {
    margin-top: 9px;
    padding-top: 7px;
    border-top: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .account-head {
    gap: 4px;
    margin-bottom: 3px;
    padding: 0 4px;
    color: var(--text);
    font-size: 7px;
  }
  .account-dot,
  .sync-dot {
    width: 4px;
    height: 4px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
  }
  .folder-row {
    gap: 5px;
    min-height: 18px;
    padding: 0 8px;
    font-size: 7px;
  }
  .folder-row :global(svg) {
    flex: none;
  }
  .folder-row .count {
    margin-left: auto;
  }
  .sidebar-foot {
    gap: 4px;
    margin-top: auto;
    padding: 6px 4px 0;
    border-top: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    color: var(--muted);
    font-size: 6.5px;
  }
  .sidebar-foot :global(svg:nth-last-child(2)) {
    margin-left: auto;
  }
  .sync-dot {
    background: var(--success);
  }
  .list {
    display: flex;
    flex-direction: column;
    min-width: 0;
    border-right: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .list-head {
    flex: none;
    padding: 7px 7px 5px;
    background: var(--bg-raised);
    border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .search-row {
    gap: 5px;
    margin-bottom: 6px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 1;
    min-width: 0;
    height: 19px;
    padding: 0 6px;
    border: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
    border-radius: 5px;
    background: color-mix(in srgb, var(--bg) 55%, var(--bg-raised));
    color: var(--muted);
    font-size: 7px;
  }
  .search :global(svg),
  .compose :global(svg) {
    flex: none;
  }
  .compose {
    display: grid;
    place-items: center;
    width: 19px;
    height: 19px;
    flex: none;
    border: 1px solid color-mix(in srgb, var(--accent) 30%, var(--border));
    border-radius: 5px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
  }
  .tabs {
    gap: 3px;
    overflow: hidden;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 16px;
    padding: 0 5px;
    border: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    border-radius: 99px;
    color: var(--muted);
    font-size: 6.5px;
    font-weight: 600;
    white-space: nowrap;
  }
  .tab.active {
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    background: color-mix(in srgb, var(--accent) 13%, transparent);
    color: var(--text-strong);
  }
  .tab.all :global(svg) {
    color: var(--accent);
  }
  .tab.primary :global(svg) {
    color: #6da4ff;
  }
  .tab.updates :global(svg) {
    color: #58c98b;
  }
  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 0;
    min-height: 44px;
    padding: 0;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 38%, transparent);
    cursor: default;
  }
  .row::before {
    content: "";
    position: absolute;
    top: 5px;
    bottom: 5px;
    left: 2px;
    width: 2px;
    border-radius: 99px;
    background: color-mix(in srgb, var(--accent) 58%, transparent);
  }
  .row.selected {
    background: color-mix(in srgb, var(--text) 5%, var(--bg-raised));
  }
  .row.selected::before {
    background: var(--accent);
  }
  .row.unread .sender,
  .row.unread .subject {
    color: var(--text-strong);
  }
  .unread-dot {
    width: 4px;
    height: 4px;
    margin: 0 3px 0 5px;
    flex: none;
    border-radius: 50%;
    background: transparent;
  }
  .row.unread .unread-dot {
    background: var(--accent);
  }
  .row-avatar {
    display: grid;
    place-items: center;
    width: 19px;
    height: 19px;
    flex: none;
    margin-left: 5px;
    margin-right: 5px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--accent) 16%, var(--bg-hover));
    color: var(--text-strong);
    font-size: 6.5px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  .row-avatar.unread {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 78%, transparent);
  }
  .compact-list .snippet {
    display: none;
  }
  .icon-tabs .tab {
    padding: 0 4px;
  }
  .row-main {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
    padding: 5px 0;
  }
  .row-top {
    display: flex;
    align-items: baseline;
    gap: 4px;
    min-width: 0;
  }
  .sender {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--text);
    font-size: 7.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .time {
    flex: none;
    color: var(--muted);
    font-size: 6.5px;
  }
  .subject {
    display: block;
    overflow: hidden;
    color: var(--text);
    font-size: 7.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .snippet {
    display: block;
    overflow: hidden;
    color: var(--muted);
    font-size: 6.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-meta {
    flex: none;
    margin-right: 5px;
    color: var(--muted);
  }
  article {
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
  }
  .reader-head {
    flex: none;
    padding: 8px 9px 7px;
    background: var(--bg-raised);
    border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .subject-row {
    gap: 5px;
    min-width: 0;
  }
  .back {
    color: var(--muted);
  }
  h1 {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    color: var(--text-strong);
    font-size: 8.5px;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .aura {
    color: var(--muted);
  }
  .reply {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 16px;
    padding: 0 5px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
    font-size: 6.5px;
    font-weight: 650;
    white-space: nowrap;
  }
  .byline {
    gap: 6px;
    margin-top: 7px;
  }
  .avatar {
    display: grid;
    place-items: center;
    width: 19px;
    height: 19px;
    flex: none;
    border-radius: 50%;
    background: color-mix(in srgb, var(--accent) 16%, var(--bg-hover));
    color: var(--text-strong);
    font-size: 6.5px;
    font-weight: 700;
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
    line-height: 1.3;
  }
  .who strong {
    overflow: hidden;
    color: var(--text);
    font-size: 7.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .who span {
    overflow: hidden;
    color: var(--muted);
    font-size: 6.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tag {
    align-self: flex-start;
    flex: none;
    padding: 2px 4px;
    border: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
    border-radius: 4px;
    color: var(--muted);
    font-size: 6px;
  }
  .reader-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-height: 0;
    padding: 9px;
    overflow: hidden;
  }
  .attachments {
    gap: 4px;
    width: fit-content;
    max-width: 100%;
    padding: 4px 6px;
    border: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    border-radius: 4px;
    color: var(--muted);
    font-size: 6.5px;
  }
  .attachments span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  article p {
    margin: 0;
    color: var(--text);
    font-size: 7px;
    line-height: 1.5;
  }
  .mail.modern {
    border-color: color-mix(in srgb, var(--text) 12%, transparent);
    background: var(--bg-app);
  }
  .mail.modern .sidebar,
  .mail.modern .rail {
    background: color-mix(in srgb, var(--bg-raised) 72%, transparent);
  }
  .mail.modern .list {
    background: color-mix(in srgb, var(--bg) 78%, transparent);
  }
  .mail.modern article {
    background: color-mix(in srgb, var(--bg) 82%, transparent);
  }
  /* Matches the real rounded theme: the navigation floats as two rounded
     cards, while the list and reader share one continuous mail surface. */
  .mail.rounded {
    gap: 3px;
    padding: 3px;
    border: 0;
    background: var(--bg-app);
  }
  .mail.rounded .rail,
  .mail.rounded .sidebar {
    border: 1px solid color-mix(in srgb, var(--text) 12%, transparent);
    border-radius: 8px;
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--text) 7%, transparent);
  }
  .mail.rounded .list {
    margin-right: -3px;
    border: 0;
    border-right: 1px solid color-mix(in srgb, var(--text) 12%, transparent);
    border-radius: 0;
    background: color-mix(in srgb, var(--bg) 78%, transparent);
  }
  .mail.rounded article {
    margin-left: -3px;
    border: 0;
    border-radius: 0;
    background: color-mix(in srgb, var(--bg) 82%, transparent);
  }
  .focus {
    grid-template-columns: 20px 24% 0 minmax(0, 1fr);
  }
  .focus .list {
    overflow: hidden;
    opacity: 0;
    padding: 0;
    border: 0;
  }
  .stacked {
    grid-template-columns: 20px 24% minmax(0, 1fr);
    grid-template-rows: 42% 58%;
  }
  .stacked .rail,
  .stacked .sidebar {
    grid-row: 1 / 3;
  }
  .stacked .list {
    border-right: 0;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  @media (max-width: 899px) {
    .preview {
      padding: 8px;
    }
    .mail {
      grid-template-columns: 18px 22% 34% minmax(0, 1fr);
    }
    .nav-item,
    .sender,
    .subject {
      font-size: 7px;
    }
    .snippet,
    .tabs {
      display: none;
    }
  }
  @media (max-width: 639px) {
    .preview {
      border-radius: 12px;
    }
    .mail {
      grid-template-columns: 18px 24% minmax(0, 1fr);
    }
    article {
      display: none;
    }
  }
</style>
