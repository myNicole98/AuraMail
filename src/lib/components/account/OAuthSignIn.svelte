<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api } from "../../api";
  import type { Account, OAuthUiStatus } from "../../types";

  let {
    provider,
    mode,
    account = null,
    spaceId = undefined,
    onCancel,
    onConnected
  }: {
    provider: "google" | "microsoft";
    mode: "add" | "reconnect";
    account?: Account | null;
    spaceId?: number;
    onCancel: () => void;
    onConnected: (account: Account) => void;
  } = $props();

  let phase = $state<"waiting" | "connecting">("waiting");
  let error = $state<string | null>(null);
  let pending = $state(false);
  let unlisten: UnlistenFn | undefined;
  let root = $state<HTMLDivElement | null>(null);
  let authUrl = $state<string | null>(null);
  let openedUrl = $state<string | null>(null);
  let copied = $state(false);

  const label = $derived(provider === "microsoft" ? "Microsoft" : "Google");
  const missingAccount = $derived(mode === "reconnect" && !account);

  function errorMessage(value: unknown): string {
    if (typeof value === "string") return value;
    if (value instanceof Error) return value.message;
    return "Something went wrong";
  }

  async function start(): Promise<void> {
    if (missingAccount) return;
    pending = true;
    error = null;
    phase = "waiting";
    authUrl = null;
    openedUrl = null;
    copied = false;
    try {
      const result =
        mode === "reconnect" && account
          ? provider === "microsoft"
            ? await api.reconnectMicrosoftAccount(account.id)
            : await api.reconnectGoogleAccount(account.id)
          : provider === "microsoft"
            ? await api.startMicrosoftOAuth(spaceId)
            : await api.startGoogleOAuth(spaceId);
      pending = false;
      onConnected(result);
    } catch (caught) {
      pending = false;
      error = errorMessage(caught);
    }
  }

  function cancelProvider(): void {
    if (provider === "microsoft") void api.cancelMicrosoftOAuth();
    else void api.cancelGoogleOAuth();
  }

  function cancel(): void {
    pending = false;
    cancelProvider();
    onCancel();
  }

  async function copyLink(): Promise<void> {
    if (!authUrl) return;
    try {
      await navigator.clipboard.writeText(authUrl);
      copied = true;
    } catch {
      const input = root?.querySelector<HTMLInputElement>(".signin-link-input");
      input?.focus();
      input?.select();
      copied = document.execCommand("copy");
    }
  }

  $effect(() => {
    void error;
    void tick().then(() => {
      if (!root) return;
      const selector = error ? ".primary" : ".ghost";
      root.querySelector<HTMLButtonElement>(selector)?.focus();
    });
  });

  onMount(() => {
    void (async () => {
      unlisten = await listen<OAuthUiStatus>("oauth://status", ({ payload }) => {
        phase = payload.phase;
        if (payload.url) {
          authUrl = payload.url;
          if (openedUrl !== payload.url) {
            openedUrl = payload.url;
            void api.openExternal(payload.url).catch(() => {});
          }
        }
      });
      await start();
    })();
    return () => {
      unlisten?.();
      if (pending) cancelProvider();
    };
  });
</script>

<div class="oauth-signin" bind:this={root}>
  {#if provider === "microsoft"}
    <div class="provider-mark microsoft" aria-hidden="true">
      <i class="ms-red"></i><i class="ms-green"></i><i class="ms-blue"></i><i class="ms-yellow"></i>
    </div>
  {:else}
    <div class="provider-mark google" aria-hidden="true">G</div>
  {/if}

  {#if missingAccount}
    <p class="signin-status" role="status">Select a {label} account to reconnect.</p>
  {:else if error}
    <p class="signin-error" role="alert">{error}</p>
    <div class="signin-actions">
      <button type="button" class="signin-button primary" onclick={() => void start()}>
        Try again
      </button>
      <button type="button" class="signin-button ghost" onclick={cancel}>
        Cancel
      </button>
    </div>
  {:else}
    <p class="signin-status" role="status">
      {phase === "connecting" ? "Connecting…" : `Waiting for ${label}…`}
    </p>
    {#if authUrl}
      <div class="signin-link">
        <span class="signin-link-label">Browser didn't open? Copy this link:</span>
        <div class="signin-link-row">
          <input
            class="signin-link-input"
            aria-label={`${label} sign-in link`}
            readonly
            value={authUrl}
            onfocus={(event) => event.currentTarget.select()}
          />
          <button type="button" class="signin-button ghost" onclick={() => void copyLink()}>
            {copied ? "Copied" : "Copy link"}
          </button>
        </div>
      </div>
    {/if}
    <div class="signin-actions">
      <button type="button" class="signin-button ghost" onclick={cancel}>
        Cancel
      </button>
    </div>
  {/if}
</div>

<style>
  .oauth-signin {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 8px 2px;
    text-align: center;
  }
  .provider-mark {
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    border-radius: 12px;
    font-size: 19px;
    font-weight: 700;
  }
  .provider-mark.google {
    background: #fff;
    color: #1a1a1a;
    font-family: var(--font-ui);
  }
  .provider-mark.microsoft {
    grid-template-columns: repeat(2, 11px);
    grid-template-rows: repeat(2, 11px);
    gap: 3px;
    padding: 7px;
    background: transparent;
  }
  .provider-mark.microsoft i {
    display: block;
    width: 11px;
    height: 11px;
  }
  .provider-mark.microsoft .ms-red {
    background: #f25022;
  }
  .provider-mark.microsoft .ms-green {
    background: #7fba00;
  }
  .provider-mark.microsoft .ms-blue {
    background: #00a4ef;
  }
  .provider-mark.microsoft .ms-yellow {
    background: #ffb900;
  }
  .signin-status {
    margin: 0;
    color: var(--text);
    font-size: 12px;
  }
  .signin-error {
    margin: 0;
    color: var(--danger);
    font-size: 11.5px;
  }
  .signin-link {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    text-align: left;
  }
  .signin-link-label {
    color: var(--muted);
    font-size: 10px;
  }
  .signin-link-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .signin-link-input {
    min-width: 0;
    flex: 1;
    padding: 7px 8px;
    border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    border-radius: 7px;
    background: color-mix(in srgb, var(--bg) 55%, var(--bg-raised));
    color: var(--text);
    font-size: 9.5px;
  }
  .signin-actions {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }
  .signin-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 8px 15px;
    border-radius: 8px;
    font-size: 11px;
    font-weight: 650;
  }
  .signin-button.primary {
    background: var(--accent);
    color: var(--bg);
  }
  .signin-button.ghost {
    border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    color: var(--muted);
  }
  .signin-button.ghost:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
</style>
