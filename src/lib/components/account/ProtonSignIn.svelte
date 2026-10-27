<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { api } from "../../api";
  import { activeSpaceId } from "../../spaces";
  import type {
    Account,
    ProtonAuthResponse,
    ProtonAuthStart,
    ProtonAuthStatus
  } from "../../types";

  let {
    mode,
    account = null,
    spaceId,
    onCancel,
    onConnected
  }: {
    mode: "add" | "reconnect";
    account?: Account | null;
    spaceId?: number;
    onCancel: () => void;
    onConnected: (account: Account) => void;
  } = $props();

  type Step =
    | "credentials"
    | "totp"
    | "mailboxPassword"
    | "verification"
    | "accessLimited";

  let step = $state<Step>("credentials");
  let username = $state(
    untrack(() => (mode === "reconnect" && account ? account.username : ""))
  );
  let password = $state("");
  let code = $state("");
  let mailboxPassword = $state("");
  let loginId = $state<string | null>(null);
  let verificationUrl = $state("");
  let verificationMethods = $state<string[]>([]);
  let accessMessage = $state("");
  let appealUrl = $state("");
  let error = $state<string | null>(null);
  let browserError = $state<string | null>(null);
  let pending = $state(false);
  let completed = $state(false);
  let disposed = false;

  const missingAccount = $derived(mode === "reconnect" && !account);
  const statusText = $derived(
    step === "totp"
      ? "Enter the two-factor authentication code for your Proton account."
      : step === "mailboxPassword"
        ? "Enter your Proton mailbox password."
        : step === "verification"
          ? "Complete the verification step in your browser."
          : "Sign in with your Proton account."
  );

  function errorMessage(value: unknown): string {
    if (typeof value === "string") return value;
    if (value instanceof Error) return value.message;
    return "Something went wrong. Try again.";
  }

  function openUrl(url: string): void {
    if (!url) return;
    browserError = null;
    void (async () => {
      try {
        await api.openExternal(url);
      } catch {
        browserError = "Could not open the browser.";
      }
    })();
  }

  function cancelSession(id: string | null): void {
    if (id && !completed) void api.cancelProtonLogin(id).catch(() => {});
  }

  function cancelReturned(status: ProtonAuthStatus): void {
    if ("loginId" in status) cancelSession(status.loginId);
  }

  function applyStatus(status: ProtonAuthStatus): void {
    if (disposed) return;
    browserError = null;

    if (status.status === "complete") {
      if (completed) return;
      completed = true;
      loginId = null;
      password = "";
      code = "";
      mailboxPassword = "";
      error = null;
      onConnected(status.account);
      return;
    }

    if (status.status === "accessLimited") {
      step = "accessLimited";
      accessMessage = status.message;
      appealUrl = status.appealUrl;
      loginId = null;
      password = "";
      code = "";
      mailboxPassword = "";
      error = null;
      return;
    }

    loginId = status.loginId;
    error = status.error;
    password = "";
    if (status.status !== "needsTotp") code = "";
    if (status.status !== "needsMailboxPassword") mailboxPassword = "";
    if (status.status === "needsTotp") {
      step = "totp";
    } else if (status.status === "needsMailboxPassword") {
      step = "mailboxPassword";
    } else {
      step = "verification";
      verificationUrl = status.url;
      verificationMethods = status.methods;
    }
  }

  async function submitCredentials(): Promise<void> {
    if (pending || completed) return;
    const user = username.trim();
    if (!user || !password) {
      error = "Enter your Proton email and password.";
      return;
    }
    pending = true;
    error = null;
    try {
      const request: ProtonAuthStart =
        mode === "reconnect" && account
          ? {
              mode: "reconnect",
              accountId: account.id,
              username: user,
              password
            }
          : {
              mode: "add",
              spaceId: spaceId ?? activeSpaceId() ?? 1,
              username: user,
              password
            };
      const status = await api.startProtonLogin(request);
      if (disposed) {
        cancelReturned(status);
        return;
      }
      applyStatus(status);
    } catch (caught) {
      if (!disposed) error = errorMessage(caught);
    } finally {
      pending = false;
    }
  }

  async function continueLogin(response: ProtonAuthResponse): Promise<void> {
    const id = loginId;
    if (!id || pending || completed) return;
    pending = true;
    error = null;
    try {
      const status = await api.continueProtonLogin(id, response);
      if (disposed) {
        cancelReturned(status);
        return;
      }
      applyStatus(status);
    } catch (caught) {
      if (!disposed) error = errorMessage(caught);
    } finally {
      pending = false;
    }
  }

  function submitTotp(): void {
    if (!code.trim()) return;
    void continueLogin({ type: "totp", value: code.trim() });
  }

  function submitMailboxPassword(): void {
    if (!mailboxPassword) return;
    void continueLogin({ type: "mailboxPassword", value: mailboxPassword });
  }

  function submitVerificationDone(): void {
    void continueLogin({ type: "verificationDone" });
  }

  function cancel(): void {
    disposed = true;
    const id = loginId;
    loginId = null;
    cancelSession(id);
    onCancel();
  }

  onDestroy(() => {
    disposed = true;
    const id = loginId;
    loginId = null;
    cancelSession(id);
  });
</script>

<div class="proton-signin" data-testid="proton-signin">
  <div class="provider-mark proton" aria-hidden="true">
    <img src="/proton.svg" alt="" />
  </div>

  {#if missingAccount}
    <p class="proton-status" role="status">
      Select a Proton account to reconnect.
    </p>
    <div class="proton-actions">
      <button type="button" class="proton-button ghost" onclick={cancel}>
        Cancel
      </button>
    </div>
  {:else if step === "accessLimited"}
    <p class="proton-error" role="alert">{accessMessage}</p>
    {#if browserError}
      <p class="proton-error" role="alert">{browserError}</p>
    {/if}
    <a
      class="proton-appeal"
      href={appealUrl}
      onclick={(event) => {
        event.preventDefault();
        openUrl(appealUrl);
      }}
    >
      Appeal access limitation
    </a>
    <div class="proton-actions">
      <button type="button" class="proton-button ghost" onclick={cancel}>
        Cancel
      </button>
    </div>
  {:else if completed}
    <p class="proton-status" role="status">Connected.</p>
  {:else}
    <p class="proton-status" role="status">{statusText}</p>
    {#if error}
      <p class="proton-error" role="alert">{error}</p>
    {/if}

    {#if step === "credentials"}
      <form
        class="proton-form"
        onsubmit={(event) => {
          event.preventDefault();
          void submitCredentials();
        }}
      >
        <label class="proton-field">
          <span>Email</span>
          <input
            type="email"
            bind:value={username}
            autocomplete="username"
            autocapitalize="none"
            spellcheck="false"
            disabled={pending}
          />
        </label>
        <p class="proton-hint">Use your Proton email or login username.</p>
        <label class="proton-field">
          <span>Password</span>
          <input
            type="password"
            bind:value={password}
            autocomplete="current-password"
            disabled={pending}
          />
        </label>
        <div class="proton-actions">
          <button type="button" class="proton-button ghost" onclick={cancel}>
            Cancel
          </button>
          <button type="submit" class="proton-button primary" disabled={pending}>
            {pending ? "Signing in…" : "Sign in"}
          </button>
        </div>
      </form>
    {:else if step === "totp"}
      <form
        class="proton-form"
        onsubmit={(event) => {
          event.preventDefault();
          submitTotp();
        }}
      >
        <label class="proton-field">
          <span>Authentication code</span>
          <input
            bind:value={code}
            inputmode="numeric"
            autocomplete="one-time-code"
            disabled={pending}
            oninput={() => (error = null)}
          />
        </label>
        <div class="proton-actions">
          <button type="button" class="proton-button ghost" onclick={cancel}>
            Cancel
          </button>
          <button
            type="submit"
            class="proton-button primary"
            disabled={pending || !code.trim()}
          >
            {pending ? "Verifying…" : "Continue"}
          </button>
        </div>
      </form>
    {:else if step === "mailboxPassword"}
      <form
        class="proton-form"
        onsubmit={(event) => {
          event.preventDefault();
          submitMailboxPassword();
        }}
      >
        <label class="proton-field">
          <span>Mailbox password</span>
          <input
            type="password"
            bind:value={mailboxPassword}
            autocomplete="new-password"
            disabled={pending}
            oninput={() => (error = null)}
          />
        </label>
        <p class="proton-hint">
          This is the password that unlocks your Proton mailbox encryption.
        </p>
        <div class="proton-actions">
          <button type="button" class="proton-button ghost" onclick={cancel}>
            Cancel
          </button>
          <button
            type="submit"
            class="proton-button primary"
            disabled={pending || !mailboxPassword}
          >
            {pending ? "Verifying…" : "Continue"}
          </button>
        </div>
      </form>
    {:else if step === "verification"}
      <div class="proton-verification">
        <label class="proton-field">
          <span>Verification link</span>
          <input
            class="proton-link"
            aria-label="Proton verification link"
            readonly
            value={verificationUrl}
            onfocus={(event) => event.currentTarget.select()}
          />
        </label>
        <p class="proton-hint">
          Open the link, complete the check, then come back here.
          {#if verificationMethods.length > 0}
            Available methods: {verificationMethods.join(", ")}.
          {/if}
        </p>
        <button
          type="button"
          class="proton-button ghost"
          onclick={() => openUrl(verificationUrl)}
        >
          Open verification page
        </button>
        {#if browserError}
          <p class="proton-error" role="alert">{browserError}</p>
        {/if}
        <div class="proton-actions">
          <button type="button" class="proton-button ghost" onclick={cancel}>
            Cancel
          </button>
          <button
            type="button"
            class="proton-button primary"
            disabled={pending}
            onclick={submitVerificationDone}
          >
            {pending ? "Checking…" : "I've completed verification"}
          </button>
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .proton-signin {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 8px 2px;
  }
  .provider-mark {
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
  }
  .provider-mark img {
    width: 42px;
    height: 42px;
    border-radius: 12px;
  }
  .proton-status {
    margin: 0;
    color: var(--text);
    font-size: 12px;
    text-align: center;
  }
  .proton-error {
    margin: 0;
    color: var(--danger);
    font-size: 11.5px;
    text-align: center;
  }
  .proton-form,
  .proton-verification {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
  }
  .proton-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--muted);
    font-size: 10px;
    text-align: left;
  }
  .proton-field input {
    height: 30px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--bg);
    color: var(--text);
    font-size: 11.5px;
  }
  .proton-field input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .proton-field input:disabled {
    opacity: 0.65;
  }
  .proton-field input.proton-link {
    font-size: 9.5px;
  }
  .proton-hint {
    margin: 0;
    color: var(--muted);
    font-size: 10px;
    text-align: left;
  }
  .proton-appeal {
    color: var(--accent);
    font-size: 11px;
    font-weight: 650;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .proton-appeal:hover {
    color: var(--text-strong);
  }
  .proton-actions {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding-top: 2px;
  }
  .proton-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 8px 15px;
    border-radius: 8px;
    font-size: 11px;
    font-weight: 650;
  }
  .proton-button.primary {
    background: var(--accent);
    color: var(--bg);
  }
  .proton-button.ghost {
    border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    color: var(--muted);
  }
  .proton-button.ghost:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .proton-button:disabled {
    cursor: default;
    opacity: 0.6;
  }
</style>
