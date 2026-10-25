<script lang="ts">
  import { cubicOut } from "svelte/easing";
  import {
    BrainCircuit,
    CalendarClock,
    LoaderCircle,
    MessageSquare,
    PanelRightClose,
    RefreshCw,
    Reply,
    Send
  } from "lucide-svelte";
  import { api } from "../api";
  import { loadPreferences } from "../settings";
  import { app } from "../stores/app.svelte";
  import { appearance } from "../stores/appearance.svelte";
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

  function railMotion(node: Element) {
    if (appearance.reduceMotion) return { duration: 0 };
    const railWidth = node.getBoundingClientRect().width || width;
    return {
      duration: 180,
      easing: cubicOut,
      css: (t: number) =>
        `opacity: ${t}; margin-right: ${-(1 - t) * railWidth}px;`
    };
  }
</script>

<aside
  class="aura-rail"
  class:resizing
  style:width={`${width}px`}
  aria-label="Aura"
  transition:railMotion
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
    <span class="aura-mark" aria-hidden="true"><BrainCircuit size={13} /></span>
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
        <span class="setup-icon"><BrainCircuit size={18} /></span>
        <h3>Aura AI isn’t configured yet</h3>
        <p>Connect a model to get a thread brief and reply drafts here.</p>
        <button type="button" class="setup-button" onclick={openSettings}>
          Open Aura AI settings
        </button>
      </div>
    {:else if loading && !rail}
      <div class="loading-state">
        <div class="loading-status">
          <span class="loading-spinner"><LoaderCircle size={14} /></span>
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
      </section>

      {#if rail.question}
        <section class="question">
          <div class="question-top">
            Before replying
          </div>
          <p>{rail.question}</p>
          <p class="question-help" role="note">
            Aura only drafts replies. It can't send, archive, or change your mail.
          </p>
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
            Choose a reply option above, or tell Aura what to say below.
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
          placeholder="Message Aura…"
          aria-label="Message Aura"
          onkeydown={onAskKeydown}
        ></textarea>
        <button
          type="button"
          class="ask-send"
          aria-label="Send message"
          title="Send message"
          disabled={askBusy || loading || !askInput.trim() || !rail}
          onclick={() => void ask()}
        >
          <Send size={15} />
        </button>
      </div>
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
    gap: 9px;
    flex: none;
    height: 48px;
    padding: 0 10px 0 12px;
    background: var(--bg-raised);
    border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .aura-mark {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    flex: none;
    border-radius: 8px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
  }
  .aura-mark :global(svg) {
    display: block;
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
    width: 28px;
    height: 28px;
    flex: none;
    border-radius: 7px;
    color: var(--muted);
  }
  .rail-icon:hover,
  .rail-icon:focus-visible {
    color: var(--text);
    background: var(--bg-hover);
  }
  .rail-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 12px 14px 18px;
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
  .draft {
    animation: aura-content-in 160ms ease-out both;
  }
  .brief {
    padding: 13px 13px 14px;
    border: 1px solid color-mix(in srgb, var(--border) 78%, transparent);
    border-radius: 10px;
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--bg-raised) 88%, var(--bg)),
      color-mix(in srgb, var(--bg-raised) 72%, var(--bg))
    );
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
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--bg-hover);
    color: var(--muted);
    font-size: 9.5px;
    font-weight: 650;
  }
  .brief-ask {
    margin: 8px 0 0;
    color: var(--text-strong);
    font-size: 13.5px;
    font-weight: 500;
    line-height: 1.55;
  }
  .brief-summary {
    margin: 7px 0 0;
    color: var(--muted);
    font-size: 12px;
    line-height: 1.6;
  }
  .brief-meta {
    margin-top: 8px;
    color: var(--muted);
    font-size: 10.5px;
  }
  .question {
    margin: 12px 0 0;
    padding: 13px;
    border: 1px solid color-mix(in srgb, var(--accent) 20%, var(--border));
    border-radius: 10px;
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--accent) 9%, var(--bg-raised)),
      color-mix(in srgb, var(--accent) 5%, var(--bg-raised))
    );
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
  .question p {
    margin: 8px 0 11px;
    color: var(--text-strong);
    font-size: 13.5px;
    font-weight: 500;
    line-height: 1.55;
  }
  .question .question-help {
    margin: -2px 0 11px;
    color: var(--muted);
    font-size: 10.5px;
    font-weight: 400;
    line-height: 1.5;
  }
  .chips {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
  }
  .chip {
    width: 100%;
    min-height: 36px;
    padding: 8px 10px;
    border: 1px solid color-mix(in srgb, var(--border) 68%, transparent);
    border-radius: 8px;
    background: color-mix(in srgb, var(--bg-raised) 88%, transparent);
    color: var(--text);
    font-size: 11.5px;
    font-weight: 560;
    line-height: 1.45;
    text-align: left;
    transition:
      background 140ms ease,
      border-color 140ms ease,
      color 140ms ease;
  }
  .chip:hover:not(:disabled),
  .chip:focus-visible:not(:disabled) {
    border-color: color-mix(in srgb, var(--accent) 28%, var(--border));
    background: var(--bg-hover);
    color: var(--text-strong);
  }
  .chip.on {
    border-color: color-mix(in srgb, var(--accent) 38%, var(--border));
    background: color-mix(in srgb, var(--accent) 14%, var(--bg-raised));
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
    margin: 16px 0 9px;
  }
  .draft-head .rail-label {
    flex: 1;
    margin-bottom: 0;
  }
  .draft-count {
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--bg-hover);
    color: var(--muted);
    font-size: 10.5px;
    font-weight: 650;
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
    padding: 11px 12px 12px;
    border: 1px solid color-mix(in srgb, var(--border) 72%, transparent);
    border-radius: 10px;
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--bg-raised) 88%, var(--bg)),
      color-mix(in srgb, var(--bg-raised) 72%, var(--bg))
    );
    text-align: left;
    transition:
      background 140ms ease,
      border-color 140ms ease;
  }
  .draft + .draft {
    margin-top: 8px;
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
  .draft:hover:not(:disabled),
  .draft:focus-visible:not(:disabled) {
    border-color: color-mix(in srgb, var(--tone-color) 36%, var(--border));
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--tone-color) 5%, var(--bg-raised)),
      color-mix(in srgb, var(--bg-raised) 82%, var(--bg))
    );
  }
  .draft:disabled {
    cursor: default;
    opacity: 0.68;
  }
  .draft-label {
    display: inline-flex;
    align-items: center;
    width: fit-content;
    padding: 2px 7px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--tone-color) 13%, transparent);
    color: var(--tone-color);
    font-size: 9.5px;
    font-weight: 650;
  }
  .draft-text {
    display: block;
    margin-top: 7px;
    color: var(--text);
    font-size: 12.5px;
    line-height: 1.6;
  }
  .draft-hint {
    display: block;
    margin-top: 8px;
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
    border: 1px solid color-mix(in srgb, var(--border) 58%, transparent);
    border-radius: 10px 10px 10px 3px;
    background: color-mix(in srgb, var(--bg-raised) 86%, var(--bg));
    color: var(--text);
    font-size: 11.5px;
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .bubble.user {
    border-color: color-mix(in srgb, var(--accent) 18%, var(--border));
    border-radius: 10px 10px 3px 10px;
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
    border: 1px solid color-mix(in srgb, var(--border) 78%, transparent);
    border-radius: 10px;
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--bg-raised) 88%, var(--bg)),
      color-mix(in srgb, var(--bg-raised) 72%, var(--bg))
    );
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
    border: 1px solid color-mix(in srgb, var(--accent) 18%, var(--border));
    border-radius: 10px;
    background: color-mix(in srgb, var(--accent) 7%, var(--bg-raised));
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
    width: 14px;
    height: 14px;
    flex: none;
    line-height: 0;
  }
  .loading-spinner :global(svg) {
    display: block;
    transform-origin: center center;
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
    border-radius: 10px;
  }
  @keyframes shimmer {
    to { background-position: -200% 0; }
  }
  @keyframes aura-spin {
    to { transform: rotate(360deg); }
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
    padding: 11px 14px;
    background: var(--bg-raised);
    border-top: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .ask {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 42px;
    padding: 5px 5px 5px 12px;
    border: 1px solid color-mix(in srgb, var(--border) 78%, transparent);
    border-radius: 10px;
    background: var(--bg);
    transition:
      border-color 140ms ease,
      box-shadow 140ms ease;
  }
  .ask:focus-within {
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .ask textarea {
    flex: 1;
    min-width: 0;
    max-height: 88px;
    padding: 7px 0;
    border: 0;
    outline: 0;
    resize: none;
    background: transparent;
    color: var(--text);
    font-size: 12px;
    line-height: 1.45;
  }
  .ask textarea:focus-visible {
    outline: none;
  }
  .ask textarea::placeholder {
    color: var(--muted);
  }
  .ask-send {
    display: grid;
    place-items: center;
    align-self: center;
    width: 30px;
    height: 30px;
    padding: 0;
    flex: none;
    border-radius: 8px;
    background: var(--accent);
    color: var(--bg);
    line-height: 0;
    transition:
      background 140ms ease,
      color 140ms ease,
      filter 140ms ease;
  }
  .ask-send :global(svg) {
    display: block;
  }
  .ask-send:hover:not(:disabled) {
    filter: brightness(1.08);
  }
  .ask-send:focus-visible {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 28%, transparent);
  }
  .ask-send:disabled {
    cursor: default;
    background: color-mix(in srgb, var(--muted) 14%, transparent);
    color: var(--muted);
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
