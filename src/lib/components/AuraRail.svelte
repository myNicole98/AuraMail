<script lang="ts">
  import {
    CalendarClock,
    CircleHelp,
    FileText,
    LoaderCircle,
    Lock,
    MessageSquare,
    PanelRightClose,
    RefreshCw,
    Reply,
    Send,
    Sparkles
  } from "lucide-svelte";
  import { api } from "../api";
  import { loadPreferences } from "../settings";
  import { app } from "../stores/app.svelte";
  import type { AiAuraDraft, AiAuraRail, ChatMessage, MessageDetail } from "../types";

  type MessageEntry = ChatMessage & { kind: "message" };
  type ReplyEntry = { kind: "replies"; drafts: AiAuraDraft[] };
  type ConversationEntry = MessageEntry | ReplyEntry;

  let {
    detail,
    onUseDraft,
    onClose
  }: {
    detail: MessageDetail;
    onUseDraft: (text: string) => Promise<void>;
    onClose: () => void;
  } = $props();

  const DEFAULT_WIDTH = 360;
  const MIN_WIDTH = 300;
  const MAX_WIDTH = 560;
  const WIDTH_STEP = 16;
  const WIDTH_KEY = "auramail.auraRailWidth";

  function clampWidth(value: number): number {
    const max = Math.min(
      MAX_WIDTH,
      Math.max(MIN_WIDTH, typeof window === "undefined" ? MAX_WIDTH : window.innerWidth - 80)
    );
    return Math.max(MIN_WIDTH, Math.min(max, value));
  }

  function loadWidth(): number {
    try {
      const stored = Number(localStorage.getItem(WIDTH_KEY));
      return Number.isFinite(stored) && stored > 0 ? clampWidth(stored) : DEFAULT_WIDTH;
    } catch {
      return DEFAULT_WIDTH;
    }
  }

  function saveWidth(): void {
    try {
      localStorage.setItem(WIDTH_KEY, String(width));
    } catch {
      // Resizing still works for the session when storage is unavailable.
    }
  }

  let width = $state(loadWidth());
  let resizing = $state(false);
  let resizeStartX = 0;
  let resizeStartWidth = DEFAULT_WIDTH;

  let rail = $state<AiAuraRail | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let errorDetail = $state<string | null>(null);
  let answer = $state<string | null>(null);
  let loadedFor = $state<number | null>(null);
  let request = 0;

  let thread = $state<ConversationEntry[]>([]);
  let askInput = $state("");
  let askBusy = $state(false);
  let askError = $state<string | null>(null);
  let askRequest = 0;
  let hasConversationReplies = $derived(
    thread.some((entry) => entry.kind === "replies")
  );

  let usingDraft = $state<string | null>(null);
  let draftError = $state<string | null>(null);
  let conversationEnd = $state<HTMLDivElement | null>(null);

  $effect(() => {
    const id = detail.message.id;
    if (!app.aiConfigured) {
      loadedFor = null;
      rail = null;
      error = null;
      errorDetail = null;
      loading = false;
      return;
    }
    if (loadedFor === id) return;
    loadedFor = id;
    request += 1;
    rail = null;
    error = null;
    errorDetail = null;
    answer = null;
    thread = [];
    askInput = "";
    askError = null;
    askBusy = false;
    askRequest += 1;
    draftError = null;
    usingDraft = null;
    void load();
  });

  $effect(() => {
    thread.length;
    askBusy;
    if ((thread.length === 0 && !askBusy) || !conversationEnd) return;
    requestAnimationFrame(() => {
      conversationEnd?.scrollIntoView?.({ block: "nearest" });
    });
  });

  async function load(nextAnswer: string | null = answer): Promise<void> {
    const token = ++request;
    loading = true;
    error = null;
    errorDetail = null;
    draftError = null;
    usingDraft = null;
    try {
      const result = await api.aiAuraRail(
        detail.message.id,
        nextAnswer ?? undefined,
        loadPreferences().full_body_context
      );
      if (token !== request) return;
      rail = result;
      answer = nextAnswer;
    } catch (caught) {
      if (token === request) {
        error = "Aura couldn't read this thread.";
        errorDetail =
          caught instanceof Error
            ? caught.message
            : typeof caught === "string"
              ? caught
              : "Unknown AI error";
      }
    } finally {
      if (token === request) loading = false;
    }
  }

  async function ask(): Promise<void> {
    const question = askInput.trim();
    if (!question || askBusy || loading || !rail) return;
    const token = ++askRequest;
    const history = thread
      .filter((entry): entry is MessageEntry => entry.kind === "message")
      .map(({ role, content }) => ({ role, content }));
    const existing = [
      ...rail.drafts,
      ...thread.flatMap((entry) =>
        entry.kind === "replies" ? entry.drafts : []
      )
    ].map((draft) => draft.text);
    thread = [
      ...thread,
      { kind: "message", role: "user", content: question }
    ];
    askInput = "";
    askBusy = true;
    askError = null;
    try {
      const response = await api.aiAuraAsk(
        detail.message.id,
        question,
        history,
        existing,
        loadPreferences().full_body_context
      );
      if (token !== askRequest) return;
      const additions: ConversationEntry[] = [
        { kind: "message", role: "assistant", content: response.answer }
      ];
      if (response.drafts.length > 0) {
        additions.push({ kind: "replies", drafts: response.drafts });
      }
      thread = [...thread, ...additions];
      if (response.drafts.length > 0 && rail) {
        answer = null;
        rail = { ...rail, question: null, options: [] };
      }
    } catch {
      if (token === askRequest) {
        askError = "Aura couldn't answer that.";
      }
    } finally {
      if (token === askRequest) askBusy = false;
    }
  }

  async function useDraft(draft: AiAuraDraft, key: string): Promise<void> {
    if (usingDraft !== null) return;
    usingDraft = key;
    draftError = null;
    try {
      await onUseDraft(draft.text);
    } catch (caught) {
      draftError = errorMessage(caught);
    } finally {
      usingDraft = null;
    }
  }

  function onAskKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void ask();
    }
  }

  function toneLabel(tone: string): string {
    if (tone === "warm") return "Warm";
    if (tone === "concise") return "Concise";
    return "Direct";
  }

  function openSettings() {
    window.dispatchEvent(new CustomEvent("auramail:open-settings"));
  }

  function startResize(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    resizing = true;
    resizeStartX = event.clientX;
    resizeStartWidth = width;
    (event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId);
  }

  function moveResize(event: PointerEvent) {
    if (!resizing) return;
    width = clampWidth(resizeStartWidth + (resizeStartX - event.clientX));
  }

  function endResize() {
    if (!resizing) return;
    resizing = false;
    saveWidth();
  }

  function onResizeKeydown(event: KeyboardEvent) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    width = clampWidth(width + (event.key === "ArrowLeft" ? WIDTH_STEP : -WIDTH_STEP));
    saveWidth();
  }

  function resetWidth() {
    width = DEFAULT_WIDTH;
    saveWidth();
  }

  function errorMessage(value: unknown): string {
    if (typeof value === "string") return value;
    if (value instanceof Error) return value.message;
    return "Could not open the draft.";
  }
</script>

<aside
  class="aura-rail"
  class:resizing
  style:width={`${width}px`}
  aria-label="Aura"
>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="resize-handle"
    role="separator"
    aria-orientation="vertical"
    aria-label="Resize Aura"
    aria-valuenow={width}
    aria-valuemin={MIN_WIDTH}
    aria-valuemax={MAX_WIDTH}
    title="Drag to resize · double-click to reset"
    tabindex="0"
    onpointerdown={startResize}
    onpointermove={moveResize}
    onpointerup={endResize}
    onpointercancel={endResize}
    onkeydown={onResizeKeydown}
    ondblclick={resetWidth}
  ></div>
  <header class="rail-head">
    <strong>Aura</strong>
    <span class="head-spacer"></span>
    <button
      class="rail-icon"
      type="button"
      aria-label="Close Aura"
      title="Close Aura"
      onclick={onClose}
    >
      <PanelRightClose size={15} />
    </button>
  </header>

  <div class="rail-body" aria-live="polite">
    {#if !app.aiConfigured}
      <div class="setup">
        <span class="setup-icon"><Sparkles size={18} /></span>
        <h3>Aura AI isn’t configured yet</h3>
        <p>Connect a model to get a thread brief and reply drafts here.</p>
        <button type="button" class="setup-button" onclick={openSettings}>
          Open Aura AI settings
        </button>
      </div>
    {:else if loading && !rail}
      <div class="loading-state">
        <div class="loading-status">
          <span class="loading-spinner"><LoaderCircle size={13} /></span>
          Reading the thread…
        </div>
        <div class="skeleton" aria-hidden="true">
          <span class="skeleton-line short"></span>
          <span class="skeleton-line"></span>
          <span class="skeleton-line"></span>
          <span class="skeleton-card"></span>
          <span class="skeleton-card"></span>
        </div>
      </div>
    {:else if error}
      <div class="state-card">
        <p>{error}</p>
        {#if errorDetail}
          <p class="error-detail">{errorDetail}</p>
        {/if}
        <button type="button" class="retry" onclick={() => void load()}>
          <RefreshCw size={13} />
          Retry
        </button>
      </div>
    {:else if rail}
      <section class="brief">
        <div class="brief-top">
          <span class="section-icon"><FileText size={12} /></span>
          <span>Summary</span>
          {#if rail.deadline}
            <span class="deadline"><CalendarClock size={10} />{rail.deadline}</span>
          {/if}
        </div>
        <p class="brief-ask">{rail.ask}</p>
        {#if rail.summary && rail.summary !== rail.ask}
          <p class="brief-summary">{rail.summary}</p>
        {/if}
        {#if rail.tone}
          <div class="brief-meta">Tone · {rail.tone}</div>
        {/if}
        <div class="context-row">
          <span class="lock"><Lock size={11} /></span>
          <span>
            This thread · {rail.messageCount}
            {rail.messageCount === 1 ? "message" : "messages"}
          </span>
        </div>
      </section>

      {#if rail.question}
        <section class="question">
          <div class="question-top">
            <span class="question-icon"><CircleHelp size={13} /></span>
            Before replying
          </div>
          <p>{rail.question}</p>
          {#if rail.options.length > 0}
            <div class="chips">
              {#each rail.options as option, index (index)}
                <button
                  type="button"
                  class="chip"
                  class:on={answer === option}
                  disabled={loading || askBusy}
                  onclick={() => {
                    answer = option;
                    void load(option);
                  }}
                >
                  {option}
                </button>
              {/each}
            </div>
          {/if}
        </section>
      {/if}

      {#if rail.drafts.length > 0 || !hasConversationReplies}
        <div class="draft-head">
          <div class="rail-label">
            <span class="section-icon"><Reply size={12} /></span>
            Suggested replies
          </div>
          {#if rail.drafts.length > 0}
            <span class="draft-count">{rail.drafts.length} options</span>
          {/if}
        </div>

        {#if loading}
          <div class="draft-loading" role="status">
            <span class="loading-spinner"><LoaderCircle size={14} /></span>
            Drafting replies…
          </div>
        {:else if rail.drafts.length === 0}
          <p class="draft-empty">
            Answer above, or describe your reply below.
          </p>
        {:else}
          {#each rail.drafts as draft, index (index)}
            <button
              type="button"
              class="draft {draft.tone}"
              aria-label={`Use ${toneLabel(draft.tone)} reply`}
              disabled={usingDraft !== null || loading}
              onclick={() => void useDraft(draft, `rail:${index}`)}
            >
              <span class="draft-label">{toneLabel(draft.tone)}</span>
              <span class="draft-text">{draft.text}</span>
              <span class="draft-hint">
                {usingDraft === `rail:${index}` ? "Opening…" : "Use reply"}
              </span>
            </button>
          {/each}
        {/if}
      {/if}

      {#if draftError}
        <p class="inline-error" role="alert">{draftError}</p>
      {/if}

      {#if thread.length > 0 || askBusy}
        <div class="rail-label conversation-label">
          <span class="section-icon"><MessageSquare size={12} /></span>
          Conversation
        </div>
        <div class="conversation" role="log" aria-label="Conversation">
          {#each thread as entry, index (index)}
            {#if entry.kind === "replies"}
              <div class="conversation-replies">
                {#each entry.drafts as draft, draftIndex (draftIndex)}
                  <button
                    type="button"
                    class="draft {draft.tone}"
                    aria-label={`Use ${toneLabel(draft.tone)} reply`}
                    disabled={usingDraft !== null || loading}
                    onclick={() =>
                      void useDraft(draft, `chat:${index}:${draftIndex}`)}
                  >
                    <span class="draft-label">{toneLabel(draft.tone)}</span>
                    <span class="draft-text">{draft.text}</span>
                    <span class="draft-hint">
                      {usingDraft === `chat:${index}:${draftIndex}`
                        ? "Opening…"
                        : "Use reply"}
                    </span>
                  </button>
                {/each}
              </div>
            {:else}
              <div class="bubble-row" class:user={entry.role === "user"}>
                <div class="bubble" class:user={entry.role === "user"}>
                  {entry.content}
                </div>
              </div>
            {/if}
          {/each}
          {#if askBusy}
            <div class="bubble-row">
              <div class="bubble thinking" aria-label="Aura is thinking">
                <span></span><span></span><span></span>
              </div>
            </div>
          {/if}
          {#if askError}
            <p class="inline-error" role="alert">{askError}</p>
          {/if}
          <div bind:this={conversationEnd}></div>
        </div>
      {/if}
    {/if}
  </div>

  {#if app.aiConfigured}
    <footer class="rail-foot">
      <div class="ask">
        <textarea
          bind:value={askInput}
          rows="1"
          placeholder="Ask, or say how you want to reply…"
          aria-label="Ask Aura or say how you want to reply"
          onkeydown={onAskKeydown}
        ></textarea>
        <button
          type="button"
          class="ask-send"
          aria-label="Ask Aura"
          title="Ask Aura"
          disabled={askBusy || loading || !askInput.trim() || !rail}
          onclick={() => void ask()}
        >
          <Send size={13} />
        </button>
      </div>
      <div class="foot-note">Esc to close</div>
    </footer>
  {/if}
</aside>

<style>
  .aura-rail {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 360px;
    min-width: 0;
    height: 100%;
    flex: none;
    border-left: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    background: var(--bg);
    animation: aura-rail-in 180ms cubic-bezier(0.22, 0.8, 0.2, 1) both;
  }
  .aura-rail.resizing {
    user-select: none;
  }
  .resize-handle {
    position: absolute;
    top: 0;
    bottom: 0;
    left: -4px;
    z-index: 4;
    width: 8px;
    cursor: col-resize;
    touch-action: none;
  }
  .resize-handle::after {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 3px;
    width: 1px;
    background: transparent;
    transition: background 140ms ease;
  }
  .resize-handle:hover::after,
  .resize-handle:focus-visible::after,
  .aura-rail.resizing .resize-handle::after {
    background: var(--accent);
  }
  .resize-handle:focus-visible {
    outline: none;
  }
  .rail-head {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
    height: 46px;
    padding: 0 12px 0 14px;
    background: var(--bg-raised);
    border-bottom: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
  }
  .rail-head strong {
    color: var(--text-strong);
    font-size: 13.5px;
    font-weight: 680;
    letter-spacing: -0.01em;
  }
  .head-spacer {
    flex: 1;
  }
  .rail-icon {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    color: var(--muted);
  }
  .rail-icon:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .rail-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 14px 14px 20px;
  }
  .rail-label {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 8px;
    color: var(--text-strong);
    font-size: 11.5px;
    font-weight: 650;
  }
  .section-icon {
    display: grid;
    place-items: center;
    color: var(--muted);
  }
  .brief,
  .question,
  .context-row,
  .draft {
    animation: aura-content-in 160ms ease-out both;
  }
  .brief {
    padding: 0 0 14px;
  }
  .brief-top {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-strong);
    font-size: 11.5px;
    font-weight: 650;
  }
  .deadline {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
    padding: 3px 7px;
    border-radius: 4px;
    background: var(--bg-hover);
    color: var(--muted);
    font-size: 9.5px;
    font-weight: 600;
  }
  .brief-ask {
    margin: 7px 0 0;
    color: var(--text-strong);
    font-size: 13px;
    font-weight: 500;
    line-height: 1.5;
  }
  .brief-summary {
    margin: 7px 0 0;
    color: var(--muted);
    font-size: 11px;
    line-height: 1.6;
  }
  .brief-meta {
    margin-top: 7px;
    color: var(--muted);
    font-size: 10px;
  }
  .context-row {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 8px 0 0;
    padding: 0;
    color: var(--muted);
    font-size: 10px;
  }
  .lock {
    display: grid;
    place-items: center;
    flex: none;
    color: var(--success);
  }
  .question {
    margin: 16px 0 0;
    padding: 14px 14px 15px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--accent) 6%, transparent);
  }
  .question-top {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--accent);
    font-size: 11.5px;
    font-weight: 650;
    letter-spacing: 0.01em;
  }
  .question-icon {
    display: grid;
    place-items: center;
    color: var(--accent);
  }
  .question p {
    margin: 9px 0 12px;
    color: var(--text-strong);
    font-size: 14px;
    font-weight: 500;
    line-height: 1.5;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
  }
  .chip {
    min-height: 32px;
    padding: 7px 12px;
    border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    border-radius: 7px;
    background: var(--bg-raised);
    color: var(--text);
    font-size: 11.5px;
    font-weight: 600;
    line-height: 1.25;
    transition:
      background 140ms ease,
      border-color 140ms ease,
      color 140ms ease;
  }
  .chip:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-strong);
  }
  .chip.on {
    border-color: transparent;
    background: color-mix(in srgb, var(--accent) 13%, transparent);
    color: var(--accent);
  }
  .chip:disabled {
    cursor: default;
    opacity: 0.55;
  }
  .draft-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 16px 0 10px;
  }
  .draft-head .rail-label {
    flex: 1;
    margin-bottom: 0;
  }
  .draft-count {
    color: var(--muted);
    font-size: 10.5px;
  }
  .draft-empty {
    margin: 5px 0 0;
    color: var(--muted);
    font-size: 11.5px;
    line-height: 1.6;
  }
  .draft {
    --tone-color: var(--muted);
    display: block;
    width: 100%;
    margin: 0;
    padding: 11px 12px;
    border: 1px solid transparent;
    border-radius: 9px;
    background: color-mix(in srgb, var(--tone-color) 6%, transparent);
    text-align: left;
    transition: background 140ms ease;
  }
  .draft + .draft {
    margin-top: 10px;
  }
  .draft.direct {
    --tone-color: var(--info);
  }
  .draft.warm {
    --tone-color: var(--warning);
  }
  .draft.concise {
    --tone-color: var(--success);
  }
  .draft:hover:not(:disabled) {
    background: color-mix(in srgb, var(--tone-color) 12%, transparent);
  }
  .draft:focus-visible:not(:disabled) {
    background: color-mix(in srgb, var(--tone-color) 12%, transparent);
  }
  .draft:disabled {
    cursor: default;
    opacity: 0.68;
  }
  .draft-label {
    display: block;
    color: var(--tone-color);
    font-size: 10.5px;
    font-weight: 650;
  }
  .draft-text {
    display: block;
    margin-top: 5px;
    color: var(--text);
    font-size: 12px;
    line-height: 1.6;
  }
  .draft-hint {
    display: block;
    margin-top: 7px;
    color: var(--muted);
    font-size: 9.5px;
    font-weight: 600;
  }
  .draft:hover:not(:disabled) .draft-hint {
    color: var(--tone-color);
  }
  .conversation-label {
    margin-top: 20px;
  }
  .conversation {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .conversation-replies {
    margin: 3px 0 5px;
  }
  .conversation-replies .draft:first-child {
    margin-top: 0;
  }
  .bubble-row {
    display: flex;
    justify-content: flex-start;
  }
  .bubble-row.user {
    justify-content: flex-end;
  }
  .bubble {
    max-width: 88%;
    padding: 8px 10px;
    border-radius: 8px 8px 8px 2px;
    background: var(--bg-raised);
    color: var(--text);
    font-size: 11.5px;
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .bubble.user {
    border-radius: 8px 8px 2px 8px;
    background: color-mix(in srgb, var(--accent) 11%, var(--bg-raised));
    color: var(--text-strong);
  }
  .thinking {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 9px 10px;
  }
  .thinking span {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--muted);
    animation: bounce 1.2s infinite ease-in-out;
  }
  .thinking span:nth-child(2) {
    animation-delay: 0.15s;
  }
  .thinking span:nth-child(3) {
    animation-delay: 0.3s;
  }
  @keyframes bounce {
    0%, 70%, 100% { opacity: 0.35; transform: translateY(0); }
    35% { opacity: 1; transform: translateY(-2px); }
  }
  .inline-error {
    margin: 7px 0 0;
    color: var(--danger);
    font-size: 10.5px;
    line-height: 1.4;
  }
  .state-card,
  .setup {
    padding: 15px;
    border: 1px solid color-mix(in srgb, var(--border) 85%, transparent);
    border-radius: 9px;
    background: var(--bg-raised);
  }
  .state-card p,
  .setup p {
    margin: 0;
    color: var(--muted);
    font-size: 11.5px;
    line-height: 1.5;
  }
  .state-card .error-detail {
    margin-top: 5px;
    color: var(--danger);
    font-size: 10.5px;
    overflow-wrap: anywhere;
  }
  .retry,
  .setup-button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-top: 9px;
    height: 27px;
    padding: 0 9px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
    font-size: 10.5px;
    font-weight: 650;
  }
  .retry:hover,
  .setup-button:hover {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .setup {
    text-align: left;
  }
  .setup-icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    margin-bottom: 9px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    color: var(--accent);
  }
  .setup h3 {
    margin: 0 0 4px;
    color: var(--text-strong);
    font-size: 13px;
    font-weight: 650;
  }
  .loading-state {
    padding: 2px 1px;
  }
  .loading-status {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 10px;
    color: var(--muted);
    font-size: 11px;
    font-weight: 600;
  }
  .draft-loading {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 6px;
    padding: 10px 12px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--accent) 6%, transparent);
    color: var(--text-strong);
    font-size: 11.5px;
    font-weight: 600;
  }
  .draft-loading .loading-spinner {
    color: var(--accent);
  }
  .loading-spinner {
    display: grid;
    place-items: center;
    animation: aura-spin 900ms linear infinite;
  }
  .skeleton {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 2px 1px;
  }
  .skeleton-line,
  .skeleton-card {
    display: block;
    border-radius: 5px;
    background: linear-gradient(
      90deg,
      color-mix(in srgb, #fff 4%, transparent),
      color-mix(in srgb, #fff 8%, transparent),
      color-mix(in srgb, #fff 4%, transparent)
    );
    background-size: 200% 100%;
    animation: shimmer 1.4s linear infinite;
  }
  .skeleton-line {
    height: 9px;
  }
  .skeleton-line.short {
    width: 48%;
  }
  .skeleton-card {
    height: 72px;
    border-radius: 9px;
  }
  @keyframes shimmer {
    to { background-position: -200% 0; }
  }
  @keyframes aura-spin {
    to { transform: rotate(360deg); }
  }
  @keyframes aura-rail-in {
    from {
      opacity: 0;
      transform: translateX(14px);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }
  @keyframes aura-content-in {
    from {
      opacity: 0;
      transform: translateY(3px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  .rail-foot {
    flex: none;
    padding: 11px 14px 13px;
    background: var(--bg-raised);
    border-top: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
  }
  .ask {
    display: flex;
    align-items: flex-end;
    gap: 6px;
    min-height: 34px;
    padding: 4px 6px 4px 10px;
    border: 1px solid color-mix(in srgb, var(--border) 90%, transparent);
    border-radius: 7px;
    background: var(--bg);
  }
  .ask:focus-within {
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
  }
  .ask textarea {
    flex: 1;
    min-width: 0;
    max-height: 88px;
    padding: 5px 0;
    border: 0;
    outline: 0;
    resize: none;
    background: transparent;
    color: var(--text);
    font-size: 11.5px;
    line-height: 1.45;
  }
  .ask textarea::placeholder {
    color: var(--muted);
  }
  .ask-send {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    flex: none;
    border-radius: 5px;
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
  }
  .ask-send:disabled {
    cursor: default;
    opacity: 0.4;
  }
  .foot-note {
    margin-top: 7px;
    color: var(--muted);
    font-size: 10px;
  }
  @media (max-width: 1100px) {
    .aura-rail {
      position: absolute;
      top: 0;
      right: 0;
      bottom: 0;
      z-index: 6;
      max-width: 92%;
      box-shadow: -8px 0 24px color-mix(in srgb, #000 22%, transparent);
    }
  }
</style>
