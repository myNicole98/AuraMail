<script lang="ts">
  import { onMount } from "svelte";
  import {
    ArrowLeft,
    BrainCircuit,
    ChevronDown,
    Download,
    Forward,
    MoreHorizontal,
    Paperclip,
    Reply,
    ReplyAll
  } from "lucide-svelte";
  import { api } from "../api";
  import { replyDraftInput } from "../compose";
  import AttachmentThumbnail from "./AttachmentThumbnail.svelte";
  import AuraRail from "./AuraRail.svelte";
  import MailBodyFrame from "./MailBodyFrame.svelte";
  import SenderAvatar from "./SenderAvatar.svelte";
  import { formatFullDate } from "../format";
  import {
    bodyDocument,
    formatBytes,
    hasImages,
    hasQuotedContent,
    isSafeExternalUrl,
    type ThemeColors
  } from "../reader";
  import { loadPreferences, savePreferences, senderIsTrusted } from "../settings";
  import { app } from "../stores/app.svelte";
  import { appearance } from "../stores/appearance.svelte";
  import type { Attachment, ComposeMode, MessageDetail } from "../types";

  let detail = $derived(app.selectedMessage);
  let message = $derived(detail?.message ?? null);
  let sender = $derived(message ? message.from_name.trim() || message.from_addr : "");
  let subject = $derived(message ? message.subject.trim() || "(no subject)" : "Select a message");
  function recipientLabel(addresses: string[]): string {
    return addresses
      .map((address) => {
        const value = address.trim().toLowerCase();
        const account = app.accounts.find(
          (candidate) =>
            candidate.email.trim().toLowerCase() === value ||
            candidate.username.trim().toLowerCase() === value
        );
        return account?.display_name.trim() || address;
      })
      .join(", ");
  }

  let recipients = $derived(
    message ? recipientLabel(message.to_addrs) || "me" : ""
  );
  let cc = $derived(message ? recipientLabel(message.cc_addrs) : "");
  let inlineImagesFor = $state<number | null>(null);
  let inlineHtml = $state("");
  let loadingImages = $state(false);
  let replyMenuOpen = $state(false);
  let preferencesVersion = $state(0);
  let cachedImageHtml = $derived(
    message && inlineImagesFor === message.id && inlineHtml
      ? inlineHtml
      : (detail?.image_html ?? "")
  );
  let allowImages = $derived(
    message !== null && Boolean(cachedImageHtml)
  );
  let blockedImages = $derived(hasImages(detail?.html_body));
  let autoImagePolicy = $derived.by(() => {
    preferencesVersion;
    if (!message) return false;
    const preferences = loadPreferences();
    return (
      preferences.always_load_images ||
      senderIsTrusted(preferences, message.from_addr)
    );
  });
  let showImageBanner = $derived(
    blockedImages && !allowImages && !autoImagePolicy
  );
  let showThumbnails = $derived.by(() => {
    preferencesVersion;
    return loadPreferences().attachment_thumbnails;
  });
  let showProfilePictures = $derived.by(() => {
    preferencesVersion;
    return loadPreferences().profile_pictures;
  });
  let themeColors = $state<ThemeColors>(readThemeColors());
  let darkEmail = $state(loadPreferences().dark_email);
  let emailHtml = $derived(
    detail?.html_body
      ? allowImages && cachedImageHtml
        ? cachedImageHtml
        : detail.html_body
      : ""
  );
  let hasQuotes = $derived(hasQuotedContent(emailHtml));
  let bodyDocumentHtml = $derived(
    emailHtml
      ? bodyDocument(
          emailHtml,
          themeColors,
          darkEmail,
          allowImages,
          message?.subject ?? ""
        )
      : ""
  );
  // Keep the email iframe mounted while an uncached message loads. The store
  // clears selectedMessage during that fetch; retaining the last document here
  // avoids creating a new WebKit frame for every uncached message.
  let frameHtml = $state("");
  $effect(() => {
    if (bodyDocumentHtml) frameHtml = bodyDocumentHtml;
  });
  let inTrash = $derived(
    message ? app.folders.find((folder) => folder.id === message.folder_id)?.role === "trash" : false
  );
  let showLoading = $state(false);
  let showQuotes = $state(false);
  let expandedOlderId = $state<number | null>(null);
  let olderDetails = $state<Record<number, MessageDetail>>({});
  let olderLoadingId = $state<number | null>(null);
  let olderError = $state<string | null>(null);
  let olderReady = $state<Record<number, boolean>>({});
  let olderMessages = $derived(
    app.threadMessages.filter((item) => item.id !== app.selectedMessageId)
  );

  function resolveCssColor(value: string, fallback: string): string {
    if (!value) return fallback;
    const probe = document.createElement("span");
    probe.style.color = value;
    probe.style.display = "none";
    document.body.appendChild(probe);
    const resolved = getComputedStyle(probe).color;
    probe.remove();
    return resolved && resolved !== "rgba(0, 0, 0, 0)" ? resolved : fallback;
  }

  function readThemeColors(): ThemeColors {
    if (typeof window === "undefined") {
      return {
        background: "#ffffff",
        backgroundRaised: "#f5f5f5",
        text: "#111111",
        link: "#4c7dff"
      };
    }
    const styles = getComputedStyle(document.documentElement);
    const value = (name: string, base: string, fallback: string) => {
      const semantic = styles.getPropertyValue(name).trim();
      const candidate =
        semantic && !semantic.startsWith("var(")
          ? semantic
          : styles.getPropertyValue(base).trim();
      return resolveCssColor(candidate, fallback);
    };
    return {
      background: value("--bg", "--base00", "#ffffff"),
      backgroundRaised: value("--bg-raised", "--base01", "#f5f5f5"),
      text: value("--text", "--base05", "#111111"),
      link: value("--accent", "--base0D", "#4c7dff")
    };
  }

  function trustSender() {
    if (!message) return;
    const sender = message.from_addr.trim().toLowerCase();
    if (!sender) return;
    const preferences = loadPreferences();
    if (!senderIsTrusted(preferences, sender)) {
      const next = {
        ...preferences,
        trusted_senders: [...preferences.trusted_senders, sender]
      };
      savePreferences(next);
      void api
        .setMailPreferences({
          trusted_senders: next.trusted_senders,
          always_load_images: next.always_load_images,
          profile_pictures: next.profile_pictures
        })
        .catch(() => {});
      window.dispatchEvent(new CustomEvent("auramail:preferences-changed"));
    }
    void loadImages();
  }

  async function loadImages() {
    if (!message || loadingImages) return;
    const messageId = message.id;
    loadingImages = true;
    try {
      const html = await api.loadRemoteImages(messageId);
      inlineHtml = html;
      inlineImagesFor = messageId;
      app.error = null;
    } catch (error) {
      app.error = errorMessage(error);
    } finally {
      loadingImages = false;
    }
  }

  onMount(() => {
    const observer = new MutationObserver(() => {
      themeColors = readThemeColors();
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["style", "data-theme-scheme"]
    });
    const refreshPreferences = () => {
      darkEmail = loadPreferences().dark_email;
      preferencesVersion += 1;
    };
    window.addEventListener("auramail:preferences-changed", refreshPreferences);
    return () => {
      observer.disconnect();
      window.removeEventListener("auramail:preferences-changed", refreshPreferences);
    };
  });

  $effect(() => {
    app.selectedMessageId;
    expandedOlderId = null;
    olderDetails = {};
    olderLoadingId = null;
    olderError = null;
    olderReady = {};
    showQuotes = false;
  });

  $effect(() => {
    if (!app.messageLoading) {
      showLoading = false;
      return;
    }
    const timer = setTimeout(() => {
      showLoading = true;
    }, 150);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    preferencesVersion;
    const current = message;
    if (!current || loadingImages || allowImages) return;
    if (!blockedImages || !autoImagePolicy) return;
    void loadImages();
  });

  function openFrameLink(href: string) {
    if (!isSafeExternalUrl(href)) return;
    void openExternal(href);
  }

  async function openExternal(href: string) {
    app.error = null;
    try {
      await api.openExternal(href);
    } catch (error) {
      app.error = errorMessage(error);
    }
  }

  function composeOlder(mode: ComposeMode, messageId: number) {
    const loaded = olderDetails[messageId];
    if (loaded) {
      app.openCompose(mode, loaded);
      return;
    }
    void (async () => {
      try {
        const detail = await api.getMessage(messageId);
        olderDetails = { ...olderDetails, [messageId]: detail };
        app.openCompose(mode, detail);
      } catch (error) {
        olderError = errorMessage(error);
      }
    })();
  }

  async function toggleOlder(messageId: number) {
    if (expandedOlderId === messageId) {
      expandedOlderId = null;
      return;
    }
    expandedOlderId = messageId;
    olderError = null;
    olderReady = { ...olderReady, [messageId]: false };
    if (olderDetails[messageId]) return;
    olderLoadingId = messageId;
    try {
      const detail = await api.getMessage(messageId);
      olderDetails = { ...olderDetails, [messageId]: detail };
    } catch (error) {
      olderError = errorMessage(error);
    } finally {
      olderLoadingId = null;
    }
  }

  function download(attachment: Attachment) {
    void app.downloadAttachment(attachment.id);
  }

  function remove() {
    replyMenuOpen = false;
    if (inTrash) {
      if (
        window.confirm(
          "Delete this message permanently? This cannot be undone."
        )
      ) {
        void app.deleteSelected();
      }
    } else {
      void app.trashSelected();
    }
  }

  function compose(mode: ComposeMode) {
    if (detail) app.openCompose(mode, detail);
    replyMenuOpen = false;
  }

  async function useAuraDraft(text: string): Promise<void> {
    if (!detail) return;
    const draft = await api.saveDraft(
      replyDraftInput(detail.message, app.accounts, text)
    );
    const opened = await app.openDraft(draft.id);
    if (!opened) {
      throw new Error(app.error ?? "Could not open the draft.");
    }
  }

  function errorMessage(value: unknown): string {
    if (typeof value === "string") return value;
    if (value instanceof Error) return value.message;
    return "Something went wrong";
  }

</script>

<section
  class="reader"
  class:reader-focus={appearance.layoutMode === "reader" && app.selectedMessageId !== null}
  aria-label="Message reader"
>
  <div class="reader-main">
  {#key app.selectedMessageId}
  <header class="reader-head mail-fade">
    <div class="subject-row">
      <button
        class="icon-btn back"
        type="button"
        aria-label="Back to message list"
        onclick={() => app.clearSelection()}
      >
        <ArrowLeft size={16} />
      </button>
      <h1>{subject}</h1>
      <div class="actions">
        <button
          class="icon-btn aura-toggle"
          class:active={app.auraRailOpen}
          type="button"
          aria-label="Aura"
          aria-pressed={app.auraRailOpen}
          title="Aura"
          disabled={!message}
          onclick={() => app.toggleAuraRail()}
        >
          <BrainCircuit size={15} />
        </button>
        <div class="reply-split">
          <button
            class="reply-main"
            type="button"
            disabled={!message}
            onclick={() => compose("reply-all")}
          >
            Reply All
          </button>
          <button
            class="reply-toggle"
            type="button"
            aria-label="More reply actions"
            aria-haspopup="menu"
            aria-expanded={replyMenuOpen}
            disabled={!message}
            onclick={() => (replyMenuOpen = !replyMenuOpen)}
          >
            <ChevronDown size={14} />
          </button>
          {#if replyMenuOpen}
            <button
              class="menu-backdrop"
              type="button"
              aria-label="Close reply menu"
              onclick={() => (replyMenuOpen = false)}
            ></button>
            <div class="reply-menu" role="menu">
              <button type="button" role="menuitem" onclick={() => compose("reply")}>
                Reply
              </button>
              <button type="button" role="menuitem" onclick={() => compose("forward")}>
                Forward
              </button>
              <button type="button" role="menuitem" class="danger" onclick={remove}>
                {inTrash ? "Delete permanently" : "Delete"}
              </button>
            </div>
          {/if}
        </div>
      </div>
    </div>

    {#if message}
      <div class="byline">
        <SenderAvatar
          name={message.from_name}
          email={message.from_addr}
          enabled={showProfilePictures}
        />
        <div class="who">
          <strong>{sender}</strong>
          <span class="sender-address">{message.from_addr}</span>
          <span title={`to ${recipients}${cc ? ` cc ${cc}` : ""}`}>
            to {recipients}{#if cc} · cc {cc}{/if} · {formatFullDate(message.date, appearance.timeFormat)}
          </span>
        </div>
        {#if message.has_attachments}
          <span class="tag">Attachments</span>
        {/if}
      </div>
    {/if}
  </header>
  {/key}

  <div class="reader-body" aria-busy={app.messageLoading}>
    {#if !detail}
      <div class="placeholder empty-state">
        <img
          class="empty-logo"
          class:loading={showLoading}
          src="/aura_mail_icon_only.svg"
          alt="AuraMail"
        />
        {#if showLoading}<p>Loading message…</p>{/if}
      </div>
    {:else}
      {#if app.attachmentNotice}
        <p class="notice" role="status">{app.attachmentNotice}</p>
      {/if}
      {#if app.error}
        <p class="reader-error" role="alert">{app.error}</p>
      {/if}

      {#if detail.attachments.length > 0}
        <div class="attachments" aria-label="Attachments">
          {#each detail.attachments as attachment (attachment.id)}
            <button
              class="chip"
              type="button"
              title={`Download ${attachment.filename}`}
              onclick={() => download(attachment)}
            >
              <AttachmentThumbnail attachment={attachment} enabled={showThumbnails} />
              <Paperclip size={13} />
              <span class="chip-name">{attachment.filename}</span>
              <span class="chip-size">{formatBytes(attachment.size)}</span>
              <Download size={12} />
            </button>
          {/each}
        </div>
      {/if}

      {#if detail.html_body && showImageBanner}
        <div class="remote-banner">
          <span>Images are blocked to protect your privacy.</span>
          <div class="remote-actions">
            <button
              type="button"
              disabled={loadingImages}
              onclick={() => void loadImages()}
            >
              {loadingImages ? "Loading…" : "Load images"}
            </button>
            <button type="button" disabled={loadingImages} onclick={trustSender}>
              Trust sender
            </button>
          </div>
        </div>
      {/if}
    {/if}

    {#if frameHtml}
      <div class="frame-stack" class:hidden={!detail?.html_body}>
        <MailBodyFrame
          html={frameHtml}
          visible={Boolean(detail?.html_body)}
          transitionKey={message?.id ?? 0}
          onOpenLink={openFrameLink}
          {showQuotes}
        />
      </div>
    {/if}

    {#if detail}
      {#if !detail.html_body}
        {#if detail.text_body}
          <pre class="text-body">{detail.text_body}</pre>
        {:else}
          <p class="empty-body">This message has no readable content.</p>
        {/if}
      {/if}

      {#if hasQuotes}
        <div class="quote-more-row">
          <button
            class="quote-more"
            class:active={showQuotes}
            type="button"
            aria-label={showQuotes ? "Hide quoted text" : "Show quoted text"}
            aria-expanded={showQuotes}
            title={showQuotes ? "Hide quoted text" : "Show quoted text"}
            onclick={() => (showQuotes = !showQuotes)}
          >
            <MoreHorizontal size={17} />
          </button>
        </div>
      {/if}

      {#if olderMessages.length > 0}
        <section class="thread-older" aria-label="Earlier messages in this thread">
          <div class="thread-older-label">
            Earlier in this thread · {olderMessages.length}
          </div>
          {#each olderMessages as older (older.id)}
            {@const expanded = expandedOlderId === older.id}
            <article class="thread-older-item" class:open={expanded}>
              <button
                class="thread-older-head"
                type="button"
                aria-expanded={expanded}
                onclick={() => void toggleOlder(older.id)}
              >
                <SenderAvatar
                  name={older.from_name}
                  email={older.from_addr}
                  enabled={showProfilePictures}
                />
                <span class="thread-older-who">
                  <strong>{older.from_name.trim() || older.from_addr}</strong>
                  <span>{formatFullDate(older.date, appearance.timeFormat)}</span>
                </span>
                <span class="thread-older-snippet">
                  {older.subject.trim() || older.snippet || "(no subject)"}
                </span>
                <ChevronDown class={`thread-chevron ${expanded ? "open" : ""}`} size={14} />
              </button>
              {#if expanded}
                <div class="thread-older-body">
                  {#if olderDetails[older.id]}
                    {#if olderDetails[older.id].html_body}
                      {#if !olderReady[older.id]}
                        <p class="thread-loading">Opening…</p>
                      {/if}
                      <div class="older-frame" class:ready={olderReady[older.id]}>
                        <MailBodyFrame
                          html={bodyDocument(
                            olderDetails[older.id].html_body ?? "",
                            themeColors,
                            darkEmail,
                            false,
                            olderDetails[older.id].message.subject
                          )}
                          transitionKey={older.id}
                          onReady={() => {
                            olderReady = { ...olderReady, [older.id]: true };
                          }}
                          onOpenLink={openFrameLink}
                        />
                      </div>
                    {:else if olderDetails[older.id].text_body}
                      <pre class="text-body">{olderDetails[older.id].text_body}</pre>
                    {:else}
                      <p class="empty-body">This message has no readable content.</p>
                    {/if}
                  {:else if olderLoadingId === older.id}
                    <p class="thread-loading">Loading…</p>
                  {:else if olderError}
                    <p class="reader-error" role="alert">{olderError}</p>
                  {/if}
                  <div class="thread-older-actions">
                    <button type="button" onclick={() => composeOlder("reply", older.id)}>
                      <Reply size={13} />
                      Reply
                    </button>
                    <button type="button" onclick={() => composeOlder("reply-all", older.id)}>
                      <ReplyAll size={13} />
                      Reply all
                    </button>
                    <button type="button" onclick={() => composeOlder("forward", older.id)}>
                      <Forward size={13} />
                      Forward
                    </button>
                  </div>
                </div>
              {/if}
            </article>
          {/each}
        </section>
      {/if}
    {/if}
  </div>
  </div>
  {#if app.auraRailOpen && detail}
    <AuraRail
      {detail}
      onClose={() => app.closeAuraRail()}
      onUseDraft={useAuraDraft}
    />
  {/if}
</section>

<style>
  .reader {
    position: relative;
    display: flex;
    flex-direction: row;
    min-width: 0;
    height: 100%;
    overflow: hidden;
    background: var(--bg);
  }
  .reader-main {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    height: 100%;
    overflow: hidden;
    background: var(--bg);
  }
  .reader-head {
    flex: none;
    padding: 12px 18px 11px;
    background: var(--bg-raised);
    border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .subject-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  h1 {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    color: var(--text-strong);
    font-size: 15px;
    font-weight: 650;
    letter-spacing: -0.01em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .back {
    display: none;
  }
  .reader.reader-focus .back {
    display: grid;
  }
  .actions {
    display: flex;
    gap: 2px;
    flex: none;
  }
  .reply-split {
    position: relative;
    display: flex;
    align-items: center;
    border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
    border-radius: 8px;
    background: var(--bg);
  }
  .reply-main {
    height: 28px;
    padding: 0 10px;
    border-radius: 7px 0 0 7px;
    color: var(--text);
    font-size: 11.5px;
    font-weight: 600;
  }
  .reply-toggle {
    display: grid;
    place-items: center;
    width: 26px;
    height: 28px;
    border-left: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    border-radius: 0 7px 7px 0;
    color: var(--muted);
  }
  .reply-main:hover:not(:disabled),
  .reply-toggle:hover:not(:disabled) {
    background: var(--bg-hover);
  }
  .reply-main:disabled,
  .reply-toggle:disabled {
    cursor: default;
    opacity: 0.45;
  }
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 1;
    background: transparent;
  }
  .reply-menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 2;
    min-width: 150px;
    padding: 4px;
    border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
    border-radius: 8px;
    background: var(--bg-raised);
    box-shadow: 0 14px 34px color-mix(in srgb, #000 24%, transparent);
  }
  .reply-menu button {
    display: block;
    width: 100%;
    padding: 6px 8px;
    border-radius: 6px;
    color: var(--text);
    text-align: left;
    font-size: 11.5px;
  }
  .reply-menu button:hover {
    background: var(--bg-hover);
  }
  .reply-menu button.danger:hover {
    color: var(--danger);
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
    opacity: 0.35;
  }
  .aura-toggle.active {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .byline {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-top: 9px;
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.35;
  }
  .who strong {
    color: var(--text);
    font-size: 12px;
    font-weight: 600;
  }
  .who span {
    overflow: hidden;
    color: var(--muted);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .who .sender-address {
    color: color-mix(in srgb, var(--muted) 78%, transparent);
    font-size: 10.5px;
  }
  .tag {
    margin-left: auto;
    padding: 2px 7px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 13%, transparent);
    color: var(--accent);
    font-size: 10px;
    font-weight: 650;
  }
  .reader-body {
    flex: 1;
    min-height: 0;
    padding: 20px 22px 32px;
    overflow-y: auto;
    color: var(--text);
    font-size: 13.5px;
    line-height: 1.68;
  }
  .mail-fade {
    position: relative;
  }
  .reader-error,
  .notice {
    margin: 0 0 14px;
    padding: 8px 10px;
    border-radius: 8px;
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .reader-error {
    background: color-mix(in srgb, var(--danger, #e5484d) 12%, transparent);
    color: var(--danger, #e5484d);
  }
  .notice {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
  }
  .attachments {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
    margin-bottom: 16px;
  }
  .chip {
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    padding: 5px 9px;
    border: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
    border-radius: 8px;
    background: var(--bg-raised);
    color: var(--text);
    font-size: 11.5px;
  }
  .chip:hover {
    border-color: var(--accent);
    color: var(--text-strong);
  }
  .chip :global(.attachment-thumb) {
    margin-right: 1px;
  }
  .chip-name {
    overflow: hidden;
    max-width: 240px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip-size {
    color: var(--muted);
    font-size: 10.5px;
  }
  .thread-older {
    margin-top: 22px;
    border-top: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    padding-top: 12px;
  }
  .thread-older-label {
    margin-bottom: 7px;
    color: var(--muted);
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .thread-older-item {
    border: 1px solid color-mix(in srgb, var(--border) 45%, transparent);
    border-radius: 9px;
    background: var(--bg-raised);
    overflow: hidden;
  }
  .thread-older-item + .thread-older-item {
    margin-top: 6px;
  }
  .thread-older-head {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    padding: 8px 10px;
    color: var(--text);
    text-align: left;
  }
  .thread-older-head:hover {
    background: var(--bg-hover);
  }
  .thread-older-who {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.35;
  }
  .thread-older-who strong {
    overflow: hidden;
    color: var(--text);
    font-size: 11.5px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .thread-older-who span {
    color: var(--muted);
    font-size: 10px;
  }
  .thread-older-snippet {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  :global(.thread-chevron) {
    flex: none;
    color: var(--muted);
    transition: transform 140ms ease;
  }
  :global(.thread-chevron.open) {
    transform: rotate(180deg);
  }
  .thread-older-body {
    padding: 2px 12px 12px;
    border-top: 1px solid color-mix(in srgb, var(--border) 40%, transparent);
  }
  .older-frame {
    position: relative;
    background: transparent;
  }
  .older-frame::after {
    content: "";
    position: absolute;
    inset: 0;
    z-index: 1;
    pointer-events: none;
    background: var(--bg-raised);
    opacity: 1;
    transition: opacity 120ms ease-out;
  }
  .older-frame.ready::after {
    opacity: 0;
  }
  .thread-loading {
    margin: 10px 0;
    color: var(--muted);
    font-size: 12px;
  }
  .thread-older-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    padding-top: 7px;
    border-top: 1px solid color-mix(in srgb, var(--border) 35%, transparent);
  }
  .thread-older-actions button {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 8px;
    border-radius: 6px;
    color: var(--muted);
    font-size: 10.5px;
    font-weight: 600;
  }
  .thread-older-actions button:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .remote-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 14px;
    padding: 8px 10px;
    border: 1px solid color-mix(in srgb, var(--warning) 45%, transparent);
    border-radius: 8px;
    background: color-mix(in srgb, var(--warning) 10%, transparent);
    color: var(--text);
    font-size: 12px;
  }
  .remote-actions {
    display: flex;
    flex: none;
    gap: 6px;
  }
  .remote-banner button {
    flex: none;
    padding: 4px 9px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
    font-size: 11.5px;
    font-weight: 600;
  }
  .quote-more-row {
    display: flex;
    justify-content: flex-start;
    margin-top: 10px;
  }
  .quote-more {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 24px;
    padding: 0;
    line-height: 0;
    border: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    border-radius: 6px;
    color: var(--muted);
  }
  .quote-more:hover,
  .quote-more.active {
    color: var(--text);
    background: var(--bg-hover);
  }
  .frame-stack {
    display: block;
  }
  .frame-stack.hidden {
    display: none;
  }
  .text-body {
    margin: 0;
    background: transparent;
    color: var(--text);
    font-family: inherit;
    font-size: 13.5px;
    line-height: 1.68;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .empty-body {
    color: var(--muted);
    font-size: 12.5px;
  }
  .placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    max-width: 460px;
    margin: 56px auto 0;
    padding: 28px;
    border: 1px dashed color-mix(in srgb, var(--border) 70%, transparent);
    border-radius: 12px;
    color: var(--muted);
    text-align: center;
  }
  .placeholder :global(svg) {
    color: var(--accent);
  }
  .placeholder p {
    margin: 0;
    font-size: 12.5px;
  }
  .placeholder.empty-state {
    height: 100%;
    max-width: none;
    margin: 0;
    padding: 24px;
    border: 0;
    justify-content: center;
  }
  .empty-logo {
    width: min(180px, 55%);
    height: auto;
    opacity: 0.9;
  }
  .empty-logo.loading {
    animation: logo-pulse 1.2s ease-in-out infinite;
  }
  @keyframes logo-pulse {
    0%,
    100% {
      opacity: 0.9;
      transform: scale(1);
    }
    50% {
      opacity: 0.4;
      transform: scale(0.94);
    }
  }
  .placeholder.empty-state p {
    font-size: 14px;
  }
  @media (max-width: 760px) {
    .back {
      display: grid;
    }
    .reader-head {
      padding: 10px 12px;
    }
    .byline {
      margin-top: 7px;
    }
    .actions {
      flex-wrap: wrap;
      justify-content: flex-end;
    }
    .reader-body {
      padding: 16px 14px 24px;
    }
    .placeholder {
      margin-top: 24px;
    }
  }
</style>
