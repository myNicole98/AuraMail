<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { emit } from "@tauri-apps/api/event";
  import { LoaderCircle, X } from "lucide-svelte";
  import Composer from "./Composer.svelte";
  import { api } from "../api";
  import { activeSpaceId } from "../spaces";
  import { applyMatugenPalette, applyThemeSettings, currentSystemPrefersDark, loadPreferences } from "../settings";
  import { appearance } from "../stores/appearance.svelte";
  import { readWindowRoute } from "../windowRoute";
  import type { Account, ComposeMode, Draft, MessageDetail } from "../types";

  const params = readWindowRoute(window.location.search, window.location.hash).params;
  const modeParam = params.get("mode") ?? "new";
  const mode: ComposeMode =
    modeParam === "reply" || modeParam === "reply-all" || modeParam === "forward"
      ? modeParam
      : "new";
  const messageId = Number(params.get("messageId")) || null;
  const draftId = Number(params.get("draftId")) || null;

  let accounts = $state<Account[]>([]);
  let source = $state<MessageDetail | null>(null);
  let draft = $state<Draft | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let composer: Composer | undefined = $state();
  let closing = false;

  function isTauri(): boolean {
    return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  }

  async function applyTheme(): Promise<void> {
    void appearance.load();
    const preferences = loadPreferences();
    if (preferences.theme_choice === "matugen" && isTauri()) {
      try {
        const palette = await api.reloadTheme();
        if (palette) applyMatugenPalette(palette);
      } catch {
        // Keep the stored theme when Matugen is unavailable.
      }
    }
    const dark = preferences.theme_choice === "system" ? await currentSystemPrefersDark() : undefined;
    applyThemeSettings(preferences, dark);
  }

  async function closeWindow() {
    closing = true;
    if (isTauri()) {
      try {
        await getCurrentWindow().destroy();
      } catch {
        await getCurrentWindow().close();
      }
    } else {
      window.close();
    }
  }

  async function changed() {
    if (!isTauri()) return;
    try {
      await emit("compose://changed");
    } catch {
      // The window should still close if the refresh event cannot be delivered.
    }
  }

  async function close() {
    await changed();
    await closeWindow();
  }

  async function sent(accountId: number) {
    try {
      if (isTauri()) await emit("compose://sent", accountId);
    } catch {
      // Sending succeeded; do not keep the composer open for an event failure.
    } finally {
      await closeWindow();
    }
  }

  onMount(() => {
    document.title = "Compose — AuraMail";
    let unlisten: (() => void) | undefined;
    let unlistenFocus: (() => void) | undefined;
    if (isTauri()) {
      void getCurrentWindow()
        .onFocusChanged(({ payload }) => {
          document.documentElement.classList.toggle("window-inactive", !payload);
        })
        .then((stop) => {
          unlistenFocus = stop;
        });
      void getCurrentWindow()
        .onCloseRequested(async (event) => {
          if (closing) return;
          event.preventDefault();
          if (composer) {
            composer.requestClose();
          } else {
            await closeWindow();
          }
        })
        .then((stop) => {
          unlisten = stop;
        });
    }
    void (async () => {
      await applyTheme();
      try {
        const [accountList, sourceDetail, draftDetail] = await Promise.all([
          api.listAccounts(),
          messageId ? api.getMessage(messageId) : Promise.resolve(null),
          draftId ? api.getDraft(draftId) : Promise.resolve(null)
        ]);
        const spaceId = activeSpaceId();
        const sourceAccountId = sourceDetail?.message.account_id ?? null;
        const draftAccountId = draftDetail?.accountId ?? null;
        const relevantAccountId = draftAccountId ?? sourceAccountId;
        accounts = accountList.filter(
          (account) =>
            account.space_id === spaceId || account.id === relevantAccountId
        );
        source = sourceDetail;
        draft = draftDetail;
        document.title = draft ? "Draft — AuraMail" : "Compose — AuraMail";
        error = null;
      } catch (caught) {
        error = errorMessage(caught);
      } finally {
        loading = false;
      }
    })();
    return () => {
      unlisten?.();
      unlistenFocus?.();
    };
  });

  function errorMessage(value: unknown): string {
    if (typeof value === "string") return value;
    if (value instanceof Error) return value.message;
    return "Something went wrong";
  }
</script>

{#if loading}
  <div class="window-state">
    <LoaderCircle class="spin" size={22} />
    <span>Opening composer…</span>
  </div>
{:else if error}
  <div class="window-state">
    <p class="state-error" role="alert">{error}</p>
    <button type="button" class="state-close" onclick={() => void closeWindow()}>
      <X size={15} />
      Close
    </button>
  </div>
{:else}
  <Composer
    bind:this={composer}
    {mode}
    {source}
    {draft}
    {accounts}
    onClose={() => void close()}
    onSent={(accountId) => void sent(accountId)}
  />
{/if}

<style>
  .window-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    height: 100vh;
    background: var(--bg);
    color: var(--muted);
    font-size: 12.5px;
  }
  :global(.spin) {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
  .state-error {
    max-width: 420px;
    margin: 0;
    padding: 10px 12px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--danger, #e5484d) 12%, transparent);
    color: var(--danger, #e5484d);
    text-align: center;
  }
  .state-close {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 11px;
    border: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    border-radius: 8px;
    color: var(--text);
    font-size: 12px;
  }
</style>
