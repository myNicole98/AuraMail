<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    Bold,
    BrainCircuit,
    Check,
    ChevronDown,
    ImagePlus,
    Italic,
    Link2,
    List,
    ListOrdered,
    LoaderCircle,
    Paperclip,
    Palette,
    RemoveFormatting,
    PenLine,
    Send,
    Underline,
    X
  } from "lucide-svelte";
  import { api } from "../api";
  import AttachmentIcon from "./AttachmentIcon.svelte";
  import {
    appendSignature,
    base64ByteLength,
    composeFocusTarget,
    contactMatches,
    htmlToPlainText,
    recipientProblem,
    recipientToken,
    readFileAsBase64,
    replaceRecipientToken,
    replyAllRecipients,
    replyReferences,
    sanitizeComposeHtml,
    signatureHtml,
    splitRecipients,
    textToHtml
  } from "../compose";
  import { formatFullDate } from "../format";
  import { formatBytes } from "../reader";
  import { appearance } from "../stores/appearance.svelte";
  import type {
    Account,
    AttachmentUpload,
    ComposeInput,
    ComposeMode,
    Contact,
    Draft,
    InlineImageUpload,
    MessageDetail
  } from "../types";

  let {
    mode,
    source = null,
    draft = null,
    accounts,
    onClose,
    onSent
  }: {
    mode: ComposeMode;
    source?: MessageDetail | null;
    draft?: Draft | null;
    accounts: Account[];
    onClose: () => void;
    onSent: (accountId: number) => void;
  } = $props();

  const MAX_BYTES = 25 * 1024 * 1024;
  const COLORS = ["#e5484d", "#f5a524", "#30a46c", "#0b57d0", "#8e4ec6", "#111111"];
  const sourceMessage = untrack(() => source?.message ?? null);

  function initialAccountId(): number {
    return draft?.accountId ?? sourceMessage?.account_id ?? accounts[0]?.id ?? 0;
  }

  function ownAddresses(accountId: number): string[] {
    const account = accounts.find((candidate) => candidate.id === accountId);
    return [account?.email, account?.username].filter(
      (value): value is string => Boolean(value)
    );
  }

  function initialTo(): string {
    if (draft) return draft.toAddrs.join(", ");
    if (mode === "reply-all" && sourceMessage) {
      return replyAllRecipients(
        sourceMessage,
        ownAddresses(sourceMessage.account_id)
      ).to.join(", ");
    }
    if (mode === "new" || mode === "forward" || !sourceMessage) return "";
    return sourceMessage.from_addr;
  }

  function initialCc(): string {
    if (draft) return draft.ccAddrs.join(", ");
    if (mode !== "reply-all" || !sourceMessage) return "";
    return replyAllRecipients(
      sourceMessage,
      ownAddresses(sourceMessage.account_id)
    ).cc.join(", ");
  }

  function initialSubject(): string {
    if (draft) return draft.subject;
    if (!sourceMessage) return "";
    const value = sourceMessage.subject.trim();
    if (!value) return "";
    if (mode === "forward") {
      return /^fwd:/i.test(value) ? value : `Fwd: ${value}`;
    }
    if (mode === "reply" || mode === "reply-all") {
      return /^re:/i.test(value) ? value : `Re: ${value}`;
    }
    return value;
  }

  function htmlText(html: string | null | undefined): string {
    if (!html) return "";
    return new DOMParser().parseFromString(html, "text/html").body.textContent?.trim() ?? "";
  }

  function forwardedBody(): string {
    if (!sourceMessage) return "";
    const text = source?.text_body?.trim() || htmlText(source?.html_body);
    return [
      "",
      "",
      "---------- Forwarded message ----------",
      `From: ${sourceMessage.from_name.trim() || sourceMessage.from_addr}`,
      `Date: ${formatFullDate(sourceMessage.date, appearance.timeFormat)}`,
      `Subject: ${sourceMessage.subject.trim() || "(no subject)"}`,
      `To: ${sourceMessage.to_addrs.join(", ") || "me"}`,
      "",
      text
    ].join("\n");
  }

  function initialBody(): string {
    if (draft) return draft.body;
    return mode === "forward" ? forwardedBody() : "";
  }

  function signatureFor(accountId: number): string {
    const account = accounts.find((candidate) => candidate.id === accountId);
    if (!account?.signature.trim()) return "";
    return signatureHtml(account.signature, account);
  }

  function signatureOnNew(accountId: number): boolean {
    return (
      accounts.find((candidate) => candidate.id === accountId)
        ?.signature_on_new ?? true
    );
  }

  function currentSignature(): string {
    return (
      bodyInput?.querySelector<HTMLElement>("[data-auramail-signature]")
        ?.outerHTML ?? ""
    );
  }

  function textWithoutSignature(): string {
    if (!bodyInput) return body;
    const clone = bodyInput.cloneNode(true) as HTMLElement;
    clone.querySelector("[data-auramail-signature]")?.remove();
    return htmlToPlainText(clone.innerHTML);
  }

  function updateSignature(accountId: number, force = false) {
    if (!bodyInput) return;
    const signature = captureImages(signatureFor(accountId));
    const existing = bodyInput.querySelector<HTMLElement>(
      "[data-auramail-signature]"
    );
    if (existing) {
      if (signature) {
        existing.outerHTML = signature;
      } else {
        existing.remove();
      }
      syncBody();
      return;
    }
    if (!signature) return;
    if (!force && !signatureOnNew(accountId)) return;
    bodyInput.innerHTML = appendSignature(bodyInput.innerHTML, signature);
    syncBody();
  }

  function initialHtml(): string {
    if (draft?.bodyHtml) return draft.bodyHtml;
    const html = textToHtml(initialBody());
    const signature = signatureOnNew(initialAccountId())
      ? captureImages(signatureFor(initialAccountId()))
      : "";
    const body = html || '<div data-auramail-body="true"><br></div>';
    return appendSignature(body, signature);
  }

  function threadId(): string | null {
    if (!sourceMessage?.message_id) return null;
    const value = sourceMessage.message_id.trim();
    if (!value) return null;
    return value.startsWith("<") && value.endsWith(">") ? value : `<${value}>`;
  }

  const inReplyTo = untrack(
    () =>
      draft?.inReplyTo ??
      (mode === "reply" || mode === "reply-all" ? threadId() : null)
  );
  const references = untrack(() => {
    if (draft?.references) return draft.references;
    if (mode !== "reply" && mode !== "reply-all") return null;
    return replyReferences(threadId(), sourceMessage?.references ?? null);
  });

  let accountId = $state(initialAccountId());
  let to = $state(initialTo());
  let cc = $state(initialCc());
  let subject = $state(initialSubject());
  let body = $state(initialBody());
  const initialMarkup = initialHtml();
  let inlineImages = $state<InlineImageUpload[]>(
    untrack(() => draft?.inlineImages.map((image) => ({ ...image })) ?? [])
  );
  let bodyHtml = $state(captureImages(initialMarkup));
  let attachments = $state<AttachmentUpload[]>(
    untrack(() =>
      draft ? draft.attachments.map((attachment) => ({ ...attachment })) : []
    )
  );
  let draftId = $state<number | null>(untrack(() => draft?.id ?? null));
  let dirty = $state(untrack(() => mode !== "new" && !draft));
  let changeCount = 0;
  let sending = $state(false);
  let saving = $state(false);
  let closePending = false;
  let error = $state<string | null>(null);
  let fileInput: HTMLInputElement | null = null;
  let imageInput: HTMLInputElement | null = null;
  let bodyInput = $state<HTMLDivElement | null>(null);
  let toInput = $state<HTMLInputElement | null>(null);
  let fromOpen = $state(false);
  let colorOpen = $state(false);
  let aiOpen = $state(false);
  let aiInstruction = $state("");
  let aiBusy = $state(false);
  let aiError = $state<string | null>(null);
  let suggestions = $state<Contact[]>([]);
  let contactRequest = 0;
  let contactTimer: ReturnType<typeof setTimeout> | undefined;
  let suggestOpen = $state(false);
  let suggestIndex = $state(0);
  let savedRange: Range | null = null;

  let title = $derived(
    draft
      ? "Draft"
      : mode === "reply"
        ? "Reply"
        : mode === "reply-all"
          ? "Reply all"
          : mode === "forward"
            ? "Forward"
            : "New message"
  );
  let totalBytes = $derived(
    attachments.reduce(
      (sum, attachment) => sum + base64ByteLength(attachment.data),
      0
    ) +
      inlineImages.reduce((sum, image) => sum + base64ByteLength(image.data), 0)
  );
  let currentAccount = $derived(
    accounts.find((account) => account.id === accountId) ?? null
  );
  let recipientQuery = $derived(recipientToken(to));
  let visibleSuggestions = $derived(
    suggestOpen ? contactMatches(suggestions, recipientQuery) : []
  );

  function markDirty() {
    dirty = true;
    changeCount += 1;
  }

  function editorHtml(): string {
    return bodyInput?.innerHTML ?? bodyHtml;
  }

  function editorText(): string {
    return htmlToPlainText(editorHtml()) || body.trim();
  }

  function syncBody() {
    if (!bodyInput) return;
    bodyHtml = bodyInput.innerHTML;
    body = htmlToPlainText(bodyHtml);
    markDirty();
  }

  function outgoingHtml(): string {
    const template = document.createElement("div");
    template.innerHTML = editorHtml();
    template.querySelectorAll<HTMLImageElement>("img[data-inline-id]").forEach((image) => {
      const id = image.dataset.inlineId;
      if (id) image.setAttribute("src", `cid:${id}`);
    });
    return template.innerHTML;
  }

  function payload(send: boolean): ComposeInput {
    return {
      accountId,
      to: splitRecipients(to),
      cc: splitRecipients(cc),
      subject: subject.trim(),
      body: editorText(),
      htmlBody: send ? outgoingHtml() : editorHtml(),
      inlineImages,
      inReplyTo,
      references,
      attachments
    };
  }

  function errorMessage(value: unknown): string {
    if (typeof value === "string") return value;
    if (value instanceof Error) return value.message;
    return "Something went wrong";
  }

  function captureSelection() {
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0 || !bodyInput) return;
    const range = selection.getRangeAt(0);
    if (bodyInput.contains(range.commonAncestorContainer)) {
      savedRange = range.cloneRange();
    }
  }

  function restoreSelection() {
    if (!bodyInput) return;
    bodyInput.focus();
    if (!savedRange) return;
    const selection = window.getSelection();
    if (!selection) return;
    selection.removeAllRanges();
    selection.addRange(savedRange);
  }

  function insertHtml(html: string) {
    restoreSelection();
    document.execCommand("insertHTML", false, html);
    syncBody();
    captureSelection();
  }

  function format(command: string, value?: string) {
    restoreSelection();
    document.execCommand(command, false, value);
    syncBody();
    captureSelection();
  }

  function addLink() {
    const value = window.prompt("Link URL");
    if (!value) return;
    if (!/^(https?:\/\/|mailto:)/i.test(value.trim())) {
      error = "Links must start with http://, https://, or mailto:.";
      return;
    }
    format("createLink", value.trim());
  }

  function preventToolbarBlur(event: MouseEvent) {
    event.preventDefault();
  }

  function newContentId(): string {
    if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
      return crypto.randomUUID();
    }
    return `inline-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  }

  function dataImage(value: string): { mimeType: string; data: string } | null {
    const match = value.match(/^data:(image\/[a-z0-9.+-]+);base64,(.+)$/i);
    if (!match) return null;
    return { mimeType: match[1], data: match[2] };
  }

  function captureImages(html: string): string {
    const template = document.createElement("div");
    template.innerHTML = html;
    template.querySelectorAll<HTMLImageElement>("img").forEach((image) => {
      const src = image.getAttribute("src") ?? "";
      const parsed = dataImage(src);
      if (!parsed) return;
      const contentId = image.dataset.inlineId || newContentId();
      image.dataset.inlineId = contentId;
      if (!inlineImages.some((candidate) => candidate.contentId === contentId)) {
        inlineImages = [
          ...inlineImages,
          {
            name: `pasted-${inlineImages.length + 1}.${
              (parsed.mimeType.split("/")[1] || "png").split("+")[0]
            }`,
            mimeType: parsed.mimeType,
            data: parsed.data,
            contentId
          }
        ];
      }
    });
    return template.innerHTML;
  }

  async function addInlineImage(file: File) {
    if (totalBytes + file.size > MAX_BYTES) {
      error = "Attachments exceed the 25 MB limit.";
      return;
    }
    const data = await readFileAsBase64(file);
    const contentId = newContentId();
    inlineImages = [
      ...inlineImages,
      {
        name: file.name || `pasted-${inlineImages.length + 1}.png`,
        mimeType: file.type || "image/png",
        data,
        contentId
      }
    ];
    const extension = (file.type.split("/")[1] || "png").replace("jpeg", "jpg");
    const name = file.name || `pasted-${inlineImages.length}.${extension}`;
    insertHtml(
      `<img src="data:${file.type || "image/png"};base64,${data}" data-inline-id="${contentId}" alt="${name.replace(/"/g, "")}">`
    );
  }

  async function addFiles(files: File[]) {
    let total = totalBytes;
    for (const file of files) {
      if (total + file.size > MAX_BYTES) {
        error = "Attachments exceed the 25 MB limit.";
        return;
      }
      const data = await readFileAsBase64(file);
      attachments = [
        ...attachments,
        {
          name: file.name,
          mimeType: file.type || "application/octet-stream",
          data
        }
      ];
      total += file.size;
      markDirty();
    }
  }

  async function onFileChange(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const files = Array.from(input.files ?? []);
    input.value = "";
    await addFiles(files);
  }

  async function onImageChange(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const files = Array.from(input.files ?? []);
    input.value = "";
    for (const file of files) await addInlineImage(file);
  }

  async function onPaste(event: ClipboardEvent) {
    const clipboard = event.clipboardData;
    if (!clipboard) return;
    const files = Array.from(clipboard.files ?? []);
    if (files.length > 0) {
      event.preventDefault();
      const images = files.filter((file) => file.type.startsWith("image/"));
      const other = files.filter((file) => !file.type.startsWith("image/"));
      for (const image of images) await addInlineImage(image);
      if (other.length > 0) await addFiles(other);
      return;
    }
    const html = clipboard.getData("text/html");
    if (html) {
      event.preventDefault();
      insertHtml(captureImages(sanitizeComposeHtml(html)));
    }
  }

  function removeAttachment(index: number) {
    attachments = attachments.filter((_, position) => position !== index);
    markDirty();
  }

  async function saveDraft(): Promise<boolean> {
    if (saving || sending) return false;
    if (!accountId) {
      error = "Choose an account before saving.";
      return false;
    }
    saving = true;
    const savedAtChange = changeCount;
    try {
      const saved = await api.saveDraft(payload(false), draftId ?? undefined);
      draftId = saved.id;
      if (changeCount === savedAtChange) dirty = false;
      error = null;
      return true;
    } catch (caught) {
      error = errorMessage(caught);
      return false;
    } finally {
      saving = false;
      if (closePending) {
        closePending = false;
        void close();
      }
    }
  }

  async function send() {
    if (sending) return;
    error = null;
    if (!accountId) {
      error = "Choose an account before sending.";
      return;
    }
    const recipientIssue = recipientProblem(to, cc);
    if (recipientIssue) {
      error = recipientIssue;
      return;
    }
    if (totalBytes > MAX_BYTES) {
      error = "Attachments exceed the 25 MB limit.";
      return;
    }
    sending = true;
    try {
      await api.sendMessage(payload(true), draftId ?? undefined);
      onSent(accountId);
    } catch (caught) {
      error = errorMessage(caught);
    } finally {
      sending = false;
    }
  }

  function hasContent(): boolean {
    return Boolean(
      to.trim() ||
        cc.trim() ||
        subject.trim() ||
        editorText().trim() ||
        attachments.length > 0 ||
        inlineImages.length > 0
    );
  }

  async function close() {
    if (sending) return;
    if (saving) {
      closePending = true;
      return;
    }
    const needsSave = dirty && (draftId !== null || hasContent());
    if (needsSave && accountId && !(await saveDraft())) return;
    onClose();
  }

  export function requestClose(): void {
    void close();
  }

  async function runAi(
    aiMode: "rewrite" | "generate",
    instruction = aiInstruction.trim()
  ) {
    if (aiBusy) return;
    aiBusy = true;
    aiError = null;
    try {
      const signature = currentSignature();
      const response = await api.aiCompose({
        mode: aiMode,
        instruction,
        text: textWithoutSignature(),
        subject
      });
      if (aiMode === "rewrite") {
        bodyHtml = appendSignature(textToHtml(response.text), signature);
        if (bodyInput) bodyInput.innerHTML = bodyHtml;
        syncBody();
        aiOpen = false;
      } else {
        insertHtml(textToHtml(response.text));
        aiInstruction = "";
        aiOpen = false;
      }
    } catch (caught) {
      const message = errorMessage(caught);
      if (aiMode === "rewrite") {
        aiOpen = false;
        error = message;
      } else {
        aiError = message;
      }
    } finally {
      aiBusy = false;
    }
  }

  function selectContact(contact: Contact) {
    to = replaceRecipientToken(to, contact);
    suggestOpen = false;
    markDirty();
    requestAnimationFrame(() => toInput?.focus());
  }

  function onToKeydown(event: KeyboardEvent) {
    if (!suggestOpen || visibleSuggestions.length === 0) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      suggestIndex = (suggestIndex + 1) % visibleSuggestions.length;
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      suggestIndex =
        (suggestIndex - 1 + visibleSuggestions.length) % visibleSuggestions.length;
    } else if (event.key === "Enter" || event.key === "Tab") {
      event.preventDefault();
      selectContact(visibleSuggestions[suggestIndex] ?? visibleSuggestions[0]);
    } else if (event.key === "Escape") {
      event.preventDefault();
      suggestOpen = false;
    }
  }

  async function loadSuggestions(value: string) {
    const query = recipientToken(value);
    clearTimeout(contactTimer);
    if (!query) {
      suggestions = [];
      return;
    }
    const request = ++contactRequest;
    contactTimer = setTimeout(() => {
      void api
        .listContacts(query, 8)
        .then((result) => {
          if (request === contactRequest) suggestions = result;
        })
        .catch(() => {
          if (request === contactRequest) suggestions = [];
        });
    }, 120);
  }

  function onToInput(value: string) {
    to = value;
    suggestIndex = 0;
    suggestOpen = recipientToken(value).length > 0;
    void loadSuggestions(value);
    markDirty();
  }

  function focusBodyStart() {
    if (!bodyInput) return;
    bodyInput.focus();
    const selection = window.getSelection();
    if (!selection) return;
    const range = document.createRange();
    const firstBlock = bodyInput.querySelector("[data-auramail-body]");
    if (firstBlock) {
      range.setStart(firstBlock, 0);
    } else {
      range.selectNodeContents(bodyInput);
    }
    range.collapse(true);
    selection.removeAllRanges();
    selection.addRange(range);
  }

  onMount(() => {
    if (bodyInput) bodyInput.innerHTML = bodyHtml;
    if (composeFocusTarget(mode) === "body") {
      focusBodyStart();
    } else {
      toInput?.focus();
    }
    const timer = setInterval(() => {
      if (dirty && accountId && (draftId !== null || hasContent()) && !saving && !sending && !aiBusy) {
        void saveDraft();
      }
    }, 5000);
    return () => {
      clearInterval(timer);
      clearTimeout(contactTimer);
    };
  });
</script>

<section class="composer" aria-label="Compose message">
  <header class="composer-head">
    <h2>{title}</h2>
    <div class="account-picker">
      <span class="account-label">From</span>
      <button
        class="account-trigger"
        type="button"
        aria-haspopup="listbox"
        aria-expanded={fromOpen}
        onclick={() => (fromOpen = !fromOpen)}
      >
        <span>{currentAccount?.email ?? "Select account"}</span>
        <ChevronDown size={14} />
      </button>
      {#if fromOpen}
        <button
          class="menu-backdrop"
          type="button"
          aria-label="Close account menu"
          onclick={() => (fromOpen = false)}
        ></button>
        <div class="account-menu" role="listbox" aria-label="Sending account">
          {#each accounts as account (account.id)}
            <button
              type="button"
              class:active={account.id === accountId}
              role="option"
              aria-selected={account.id === accountId}
              onclick={() => {
                accountId = account.id;
                fromOpen = false;
                updateSignature(account.id);
                markDirty();
              }}
            >
              <span>{account.email}</span>
              {#if account.id === accountId}<Check size={14} />{/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <button
      class="icon-btn"
      type="button"
      aria-label="Close composer"
      title="Close"
      disabled={sending}
      onclick={() => void close()}
    >
      <X size={17} />
    </button>
  </header>

  <div class="fields">
    <label class="field to-field">
      <span>To</span>
      <div class="to-wrap">
        <input
          bind:this={toInput}
          value={to}
          autocomplete="off"
          aria-autocomplete="list"
          oninput={(event) => onToInput(event.currentTarget.value)}
          onkeydown={onToKeydown}
          onfocus={() => {
            if (recipientToken(to)) {
              suggestOpen = true;
              void loadSuggestions(to);
            }
          }}
          onblur={() => setTimeout(() => (suggestOpen = false), 150)}
        />
        {#if visibleSuggestions.length > 0}
          <div class="suggestions" role="listbox" aria-label="Recipient suggestions">
            {#each visibleSuggestions as contact, index (contact.email)}
              <button
                type="button"
                class:active={index === suggestIndex}
                role="option"
                aria-selected={index === suggestIndex}
                onmousedown={preventToolbarBlur}
                onclick={() => selectContact(contact)}
              >
                <span class="suggestion-name">{contact.name || contact.email}</span>
                {#if contact.name}<span class="suggestion-email">{contact.email}</span>{/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </label>
    <label class="field">
      <span>Cc</span>
      <input
        value={cc}
        placeholder="Optional"
        oninput={(event) => {
          cc = event.currentTarget.value;
          markDirty();
        }}
      />
    </label>
    <label class="field subject">
      <span>Subject</span>
      <input
        value={subject}
        placeholder="Subject"
        oninput={(event) => {
          subject = event.currentTarget.value;
          markDirty();
        }}
      />
    </label>
  </div>

  <div class="editor-shell">
    <div class="toolbar" role="toolbar" aria-label="Formatting">
      <button type="button" title="Bold" aria-label="Bold" onmousedown={preventToolbarBlur} onclick={() => format("bold")}>
        <Bold size={15} />
      </button>
      <button type="button" title="Italic" aria-label="Italic" onmousedown={preventToolbarBlur} onclick={() => format("italic")}>
        <Italic size={15} />
      </button>
      <button type="button" title="Underline" aria-label="Underline" onmousedown={preventToolbarBlur} onclick={() => format("underline")}>
        <Underline size={15} />
      </button>
      <span class="toolbar-sep"></span>
      <button type="button" title="Bulleted list" aria-label="Bulleted list" onmousedown={preventToolbarBlur} onclick={() => format("insertUnorderedList")}>
        <List size={15} />
      </button>
      <button type="button" title="Numbered list" aria-label="Numbered list" onmousedown={preventToolbarBlur} onclick={() => format("insertOrderedList")}>
        <ListOrdered size={15} />
      </button>
      <button type="button" title="Link" aria-label="Insert link" onmousedown={preventToolbarBlur} onclick={addLink}>
        <Link2 size={15} />
      </button>
      <button type="button" title="Insert image" aria-label="Insert image" onmousedown={preventToolbarBlur} onclick={() => imageInput?.click()}>
        <ImagePlus size={15} />
      </button>
      <div class="color-picker">
        <button
          type="button"
          title="Text color"
          aria-label="Text color"
          aria-expanded={colorOpen}
          onmousedown={preventToolbarBlur}
          onclick={() => (colorOpen = !colorOpen)}
        >
          <Palette size={15} />
        </button>
        {#if colorOpen}
          <div class="color-menu">
            {#each COLORS as color (color)}
              <button
                type="button"
                class="swatch"
                style:background={color}
                aria-label={`Text color ${color}`}
                onmousedown={preventToolbarBlur}
                onclick={() => {
                  format("foreColor", color);
                  colorOpen = false;
                }}
              ></button>
            {/each}
          </div>
        {/if}
      </div>
      <button type="button" title="Add signature" aria-label="Add signature" onmousedown={preventToolbarBlur} onclick={() => updateSignature(accountId, true)}>
        <PenLine size={15} />
      </button>
      <button type="button" title="Clear formatting" aria-label="Clear formatting" onmousedown={preventToolbarBlur} onclick={() => format("removeFormat")}>
        <RemoveFormatting size={15} />
      </button>
    </div>

    <div
      bind:this={bodyInput}
      class="body"
      contenteditable="true"
      role="textbox"
      tabindex="0"
      aria-multiline="true"
      aria-label="Message body"
      data-placeholder="Write a message…"
      oninput={syncBody}
      onpaste={(event) => void onPaste(event)}
      onkeyup={captureSelection}
      onmouseup={captureSelection}
    ></div>
  </div>

  <div class="attachments">
    {#each attachments as attachment, index (index)}
      <span class="chip">
        <AttachmentIcon
          filename={attachment.name}
          mimeType={attachment.mimeType}
          size={13}
        />
        <span class="chip-name">{attachment.name}</span>
        <span class="chip-size">{formatBytes(base64ByteLength(attachment.data))}</span>
        <button
          type="button"
          class="chip-remove"
          aria-label={`Remove ${attachment.name}`}
          title="Remove attachment"
          onclick={() => removeAttachment(index)}
        >
          <X size={12} />
        </button>
      </span>
    {/each}
    {#if totalBytes > 0}
      <span class="total">{formatBytes(totalBytes)} of 25 MB</span>
    {/if}
  </div>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  <footer class="composer-foot">
    <input
      bind:this={fileInput}
      class="file-input"
      type="file"
      multiple
      aria-label="Attach files"
      onchange={(event) => void onFileChange(event)}
    />
    <input
      bind:this={imageInput}
      class="file-input"
      type="file"
      accept="image/*"
      multiple
      aria-label="Insert images"
      onchange={(event) => void onImageChange(event)}
    />
    <button class="send" type="button" disabled={sending || saving} onclick={() => void send()}>
      <Send size={15} />
      {sending ? "Sending…" : "Send"}
    </button>
    <button
      class="icon-action"
      type="button"
      aria-label="Attach files"
      title="Attach files"
      onclick={() => fileInput?.click()}
    >
      <Paperclip size={16} />
    </button>
    <span class="foot-spacer"></span>
    <button
      class="ai-draft"
      class:active={aiOpen}
      type="button"
      aria-haspopup="dialog"
      aria-expanded={aiOpen}
      disabled={aiBusy}
      onmousedown={preventToolbarBlur}
      onclick={() => {
        captureSelection();
        aiOpen = !aiOpen;
        aiError = null;
      }}
    >
      {#if aiBusy}<LoaderCircle class="spin" size={15} />{:else}<BrainCircuit size={15} />{/if}
      AI Draft
    </button>
    <button
      class="ghost rewrite"
      type="button"
      disabled={aiBusy || !body.trim()}
      onclick={() => void runAi("rewrite", "")}
    >
      <PenLine size={15} />
      Rewrite
    </button>

    {#if aiOpen}
      <div class="ai-popover" role="dialog" aria-label="AI Draft">
        <textarea
          bind:value={aiInstruction}
          placeholder="Describe what to write…"
          rows="3"
          onkeydown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              if (aiInstruction.trim()) void runAi("generate");
            }
          }}
        ></textarea>
        {#if aiError}<p class="ai-error" role="alert">{aiError}</p>{/if}
      </div>
    {/if}
  </footer>
</section>

<style>
  .composer {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-width: 0;
    background: var(--bg);
  }
  .composer-head {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: none;
    padding: 12px 16px;
    background: var(--bg-raised);
    border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  h2 {
    flex: 1;
    margin: 0;
    color: var(--text-strong);
    font-size: 14px;
    font-weight: 650;
  }
  .account-picker {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    font-size: 11px;
  }
  .account-label {
    font-weight: 600;
  }
  .account-trigger {
    display: flex;
    align-items: center;
    gap: 7px;
    max-width: 250px;
    height: 30px;
    padding: 0 9px;
    border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
    border-radius: 7px;
    background: var(--bg-raised);
    color: var(--text);
    font-size: 11.5px;
  }
  .account-trigger > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .account-trigger :global(svg) {
    color: var(--muted);
  }
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 1;
    background: transparent;
  }
  .account-menu {
    position: absolute;
    top: calc(100% + 5px);
    right: 0;
    z-index: 2;
    min-width: 230px;
    padding: 4px;
    border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
    border-radius: 9px;
    background: var(--bg-raised);
    box-shadow: 0 14px 34px color-mix(in srgb, #000 24%, transparent);
  }
  .account-menu button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 32px;
    padding: 0 8px;
    border-radius: 6px;
    color: var(--text);
    text-align: left;
    font-size: 12px;
  }
  .account-menu button:hover,
  .account-menu button.active {
    background: var(--bg-hover);
  }
  .account-menu button > span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
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
  .icon-btn:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .fields {
    flex: none;
    padding: 10px 16px 4px;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 5px 0;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 40%, transparent);
  }
  .field > span {
    width: 46px;
    flex: none;
    color: var(--muted);
    font-size: 11px;
    font-weight: 600;
  }
  .field input {
    flex: 1;
    min-width: 0;
    height: 28px;
    border: 0;
    outline: 0;
    background: none;
    color: var(--text);
    font-size: 12.5px;
  }
  .to-wrap {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  .to-wrap input {
    width: 100%;
  }
  .suggestions {
    position: absolute;
    top: calc(100% + 5px);
    left: 0;
    right: 0;
    z-index: 5;
    max-height: 230px;
    overflow-y: auto;
    padding: 4px;
    border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
    border-radius: 9px;
    background: var(--bg-raised);
    box-shadow: 0 14px 34px color-mix(in srgb, #000 24%, transparent);
  }
  .suggestions button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    width: 100%;
    padding: 6px 8px;
    border-radius: 6px;
    text-align: left;
  }
  .suggestions button:hover,
  .suggestions button.active {
    background: var(--bg-hover);
  }
  .suggestion-name {
    color: var(--text);
    font-size: 12px;
  }
  .suggestion-email {
    color: var(--muted);
    font-size: 10.5px;
  }
  .editor-shell {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    margin: 6px 16px 0;
    border: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    border-radius: 9px;
    background: var(--bg-raised);
    overflow: visible;
  }
  .editor-shell:focus-within {
    border-color: var(--accent);
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: none;
    padding: 5px 6px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 45%, transparent);
  }
  .toolbar button {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    color: var(--muted);
  }
  .toolbar button:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .toolbar-sep {
    width: 1px;
    height: 18px;
    margin: 0 3px;
    background: color-mix(in srgb, var(--border) 65%, transparent);
  }
  .color-picker {
    position: relative;
  }
  .color-menu {
    position: absolute;
    top: calc(100% + 5px);
    left: 0;
    z-index: 4;
    display: flex;
    gap: 5px;
    padding: 6px;
    border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
    border-radius: 9px;
    background: var(--bg-raised);
    box-shadow: 0 12px 28px color-mix(in srgb, #000 22%, transparent);
  }
  .swatch {
    width: 20px !important;
    height: 20px !important;
    border: 1px solid color-mix(in srgb, var(--border) 60%, transparent) !important;
    border-radius: 50% !important;
  }
  .ai-popover {
    position: absolute;
    right: 16px;
    bottom: 56px;
    z-index: 10;
    width: min(360px, calc(100% - 32px));
    padding: 10px;
    border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
    border-radius: 10px;
    background: var(--bg-raised);
    box-shadow: 0 16px 40px color-mix(in srgb, #000 26%, transparent);
  }
  .ai-popover textarea {
    width: 100%;
    min-height: 62px;
    margin-top: 8px;
    padding: 8px 9px;
    border: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    border-radius: 7px;
    outline: 0;
    background: var(--bg);
    color: var(--text);
    font-family: inherit;
    font-size: 12px;
    line-height: 1.45;
    resize: vertical;
  }
  .ai-popover textarea:focus {
    border-color: var(--accent);
  }
  .ai-error {
    margin: 8px 0 0;
    color: var(--danger, #e5484d);
    font-size: 11.5px;
  }
  .body {
    flex: 1;
    min-height: 180px;
    padding: 11px 12px;
    border: 0;
    outline: 0;
    overflow-y: auto;
    color: var(--text);
    font-size: 13px;
    line-height: 1.6;
    overflow-wrap: anywhere;
  }
  .editor-shell .body:focus-visible {
    outline: none;
    box-shadow: none;
  }
  .body:empty::before {
    content: attr(data-placeholder);
    color: var(--muted);
    pointer-events: none;
  }
  .body :global(img) {
    max-width: 100%;
    height: auto;
  }
  .body :global(a) {
    color: var(--accent);
  }
  .body :global([data-auramail-body]) {
    min-height: 1.6em;
    outline: 0;
  }
  .body :global([data-auramail-signature]) {
    margin: 0;
    padding: 0;
    background: transparent;
  }
  :global(html[data-theme-dark="true"]) .body :global([data-auramail-signature] *),
  :global(html[data-theme-dark="true"]) .body :global([data-auramail-signature]) {
    background: transparent !important;
    color: var(--text) !important;
  }
  :global(html[data-theme-dark="true"]) .body :global([data-auramail-signature] a) {
    color: var(--accent) !important;
  }
  :global(.spin) {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
  .attachments {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 7px;
    flex: none;
    padding: 10px 16px 0;
  }
  .chip {
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    padding: 4px 8px;
    border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
    border-radius: 8px;
    background: var(--bg-raised);
    color: var(--text);
    font-size: 11px;
  }
  .chip-name {
    overflow: hidden;
    max-width: 220px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip-size,
  .total {
    color: var(--muted);
    font-size: 10.5px;
  }
  .chip-remove {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 5px;
    color: var(--muted);
  }
  .chip-remove:hover {
    color: var(--danger, #e5484d);
    background: var(--bg-hover);
  }
  .error {
    flex: none;
    margin: 10px 16px 0;
    padding: 8px 10px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--danger, #e5484d) 12%, transparent);
    color: var(--danger, #e5484d);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .composer-foot {
    position: relative;
    display: flex;
    align-items: center;
    gap: 7px;
    flex: none;
    padding: 12px 16px;
  }
  .foot-spacer {
    flex: 1;
  }
  .file-input {
    display: none;
  }
  .ghost,
  .send {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 11px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 600;
  }
  .ghost {
    color: var(--muted);
    border: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  }
  .ghost:hover:not(:disabled) {
    color: var(--text);
    background: var(--bg-hover);
  }
  .send {
    background: var(--accent);
    color: var(--bg);
  }
  .send:hover:not(:disabled) {
    filter: brightness(1.06);
  }
  .icon-action {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    flex: none;
    border: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    border-radius: 8px;
    color: var(--text);
  }
  .icon-action:hover {
    background: var(--bg-hover);
  }
  .ai-draft {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 12px;
    border: 1px solid color-mix(in srgb, var(--accent) 38%, transparent);
    border-radius: 8px;
    background: color-mix(in srgb, var(--accent) 13%, transparent);
    color: var(--accent);
    font-size: 12px;
    font-weight: 650;
  }
  .ai-draft:hover:not(:disabled),
  .ai-draft.active {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .ai-draft:disabled {
    opacity: 0.55;
  }
  .rewrite {
    color: var(--text);
    background: var(--bg-raised);
  }
  .rewrite:hover:not(:disabled) {
    background: var(--bg-hover);
  }
  .ghost:disabled,
  .send:disabled,
  .icon-action:disabled,
  .ai-draft:disabled,
  .rewrite:disabled {
    cursor: default;
    opacity: 0.5;
  }
  @media (max-width: 620px) {
    .account-label {
      display: none;
    }
    .account-trigger {
      max-width: 150px;
    }
  }
</style>
