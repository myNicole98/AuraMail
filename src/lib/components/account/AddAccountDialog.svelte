<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { Check, RefreshCw } from "lucide-svelte";
  import { api } from "../../api";
  import { validateAccountForm, type AccountForm } from "../../settings";
  import type { Account, AccountInput } from "../../types";
  import OAuthSignIn from "./OAuthSignIn.svelte";
  import ProtonSignIn from "./ProtonSignIn.svelte";
  import ProviderChooser from "./ProviderChooser.svelte";
  import ThemedSelect from "../ThemedSelect.svelte";

  let {
    mode,
    account = null,
    spaceId = undefined,
    onClose,
    onConnected
  }: {
    mode: "add" | "reconnect";
    account?: Account | null;
    spaceId?: number;
    onClose: () => void;
    onConnected: (account: Account) => void;
  } = $props();

  let override = $state<
    "chooser" | "google" | "microsoft" | "proton" | "manual" | null
  >(null);
  const view = $derived(
    override ??
      (mode === "add"
        ? "chooser"
        : account?.auth_type === "proton"
          ? "proton"
          : account?.oauth_provider === "microsoft"
            ? "microsoft"
            : "google")
  );
  let panel = $state<HTMLElement | null>(null);
  let previousFocus: HTMLElement | null = null;
  let form = $state<AccountForm>(blankForm());
  let errors = $state<Record<string, string>>({});
  let saving = $state(false);
  let discovering = $state(false);
  let detected = $state(false);
  let manualError = $state<string | null>(null);

  const title = $derived(mode === "add" ? "Add account" : "Reconnect account");

  function blankForm(): AccountForm {
    return {
      display_name: "",
      username: "",
      password: "",
      imap_host: "",
      imap_port: "993",
      imap_security: "tls",
      smtp_host: "",
      smtp_port: "465",
      smtp_security: "tls",
      color: "#a8c7fa"
    };
  }

  function openGoogle(): void {
    override = "google";
  }

  function openMicrosoft(): void {
    override = "microsoft";
  }

  function openManual(): void {
    override = "manual";
  }

  function openProton(): void {
    override = "proton";
  }

  function cancelOAuth(): void {
    if (mode === "add") {
      override = "chooser";
    } else {
      onClose();
    }
  }

  async function autodetect(): Promise<void> {
    if (!form.username.includes("@")) {
      manualError = "Enter an email address first.";
      return;
    }
    discovering = true;
    detected = false;
    manualError = null;
    try {
      const found = await api.discoverAccountSettings(form.username.trim());
      form = {
        ...form,
        imap_host: found.imap_host,
        imap_port: String(found.imap_port),
        imap_security: found.imap_security,
        smtp_host: found.smtp_host,
        smtp_port: String(found.smtp_port),
        smtp_security: found.smtp_security
      };
      errors = {};
      detected = true;
    } catch (error) {
      manualError = error instanceof Error ? error.message : String(error);
    } finally {
      discovering = false;
    }
  }

  async function addManual(): Promise<void> {
    const found = validateAccountForm(form, true);
    errors = found;
    manualError = null;
    if (Object.keys(found).length > 0) return;
    saving = true;
    try {
      const input: AccountInput = {
        display_name: form.display_name.trim(),
        email: form.username.trim(),
        username: form.username.trim(),
        password: form.password,
        imap_host: form.imap_host.trim(),
        imap_port: Number(form.imap_port),
        imap_security: form.imap_security,
        smtp_host: form.smtp_host.trim(),
        smtp_port: Number(form.smtp_port),
        smtp_security: form.smtp_security,
        color: form.color,
        signature: "",
        signature_fields: {
          profession: "",
          phone: "",
          location: "",
          website: "",
          socials: []
        },
        space_id: spaceId ?? 1
      };
      onConnected(await api.addAccount(input));
    } catch (error) {
      manualError = error instanceof Error ? error.message : String(error);
    } finally {
      saving = false;
    }
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.stopPropagation();
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key === "Tab") {
      trapTab(event);
    }
  }

  function trapTab(event: KeyboardEvent): void {
    if (!panel) return;
    const focusables = Array.from(
      panel.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
      )
    );
    if (focusables.length === 0) {
      event.preventDefault();
      return;
    }
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    const active = document.activeElement;
    if (event.shiftKey) {
      if (active === first || !panel.contains(active)) {
        event.preventDefault();
        last.focus();
      }
    } else if (active === last || !panel.contains(active)) {
      event.preventDefault();
      first.focus();
    }
  }

  function handleBackdropClick(event: MouseEvent): void {
    if (event.target === event.currentTarget) onClose();
  }

  function focusFirst(): void {
    if (!panel) return;
    const first = panel.querySelector<HTMLElement>("button:not([disabled])");
    if (first) {
      first.focus();
    } else {
      panel.focus();
    }
  }

  onMount(() => {
    previousFocus =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
  });

  $effect(() => {
    void view;
    void tick().then(() => focusFirst());
  });

  onDestroy(() => {
    previousFocus?.focus();
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="dialog-backdrop"
  role="presentation"
  onclick={handleBackdropClick}
>
  <div
    class="dialog-panel"
    role="dialog"
    aria-modal="true"
    aria-labelledby="add-account-title"
    tabindex="-1"
    onkeydown={handleKeydown}
    bind:this={panel}
  >
    <h2 class="dialog-title" id="add-account-title">{title}</h2>
    {#if view === "chooser"}
      <ProviderChooser
        onGoogle={openGoogle}
        onMicrosoft={openMicrosoft}
        onProton={openProton}
        onManual={openManual}
      />
    {:else if view === "google"}
      <OAuthSignIn
        provider="google"
        {mode}
        {account}
        {spaceId}
        onCancel={cancelOAuth}
        onConnected={onConnected}
      />
    {:else if view === "microsoft"}
      <OAuthSignIn
        provider="microsoft"
        {mode}
        {account}
        {spaceId}
        onCancel={cancelOAuth}
        onConnected={onConnected}
      />
    {:else if view === "proton"}
      <ProtonSignIn
        {mode}
        {account}
        {spaceId}
        onCancel={cancelOAuth}
        onConnected={onConnected}
      />
    {:else}
      <form
        class="manual-form"
        oninput={() => (detected = false)}
        onsubmit={(event) => {
          event.preventDefault();
          void addManual();
        }}
      >
        <label class="field">
          <span>Email</span>
          <div class="with-action">
            <input
              type="email"
              bind:value={form.username}
              autocomplete="username"
              onblur={() => {
                if (!form.imap_host.trim() && form.username.includes("@")) {
                  void autodetect();
                }
              }}
            />
            <div class="detect-control">
              <button
                type="button"
                class="ghost compact"
                disabled={discovering}
                onclick={() => void autodetect()}
              >
                <RefreshCw size={14} />
                {discovering ? "Detecting…" : "Autodetect"}
              </button>
              {#if detected}
                <span
                  class="detected-check"
                  role="status"
                  aria-label="Mail settings detected"
                >
                  <Check size={15} />
                </span>
              {/if}
            </div>
          </div>
          {#if errors.username}<small class="error">{errors.username}</small>{/if}
        </label>
        <label class="field">
          <span>Password</span>
          <input type="password" bind:value={form.password} autocomplete="new-password" />
          {#if errors.password}<small class="error">{errors.password}</small>{/if}
        </label>
        <label class="field">
          <span>Display name</span>
          <input bind:value={form.display_name} autocomplete="name" />
          {#if errors.display_name}<small class="error">{errors.display_name}</small>{/if}
        </label>
        <div class="grid">
          <label class="field">
            <span>IMAP host</span>
            <input bind:value={form.imap_host} />
            {#if errors.imap_host}<small class="error">{errors.imap_host}</small>{/if}
          </label>
          <label class="field">
            <span>Port</span>
            <input inputmode="numeric" bind:value={form.imap_port} />
            {#if errors.imap_port}<small class="error">{errors.imap_port}</small>{/if}
          </label>
        </div>
        <div class="grid">
          <label class="field">
            <span>SMTP host</span>
            <input bind:value={form.smtp_host} />
            {#if errors.smtp_host}<small class="error">{errors.smtp_host}</small>{/if}
          </label>
          <label class="field">
            <span>Port</span>
            <input inputmode="numeric" bind:value={form.smtp_port} />
            {#if errors.smtp_port}<small class="error">{errors.smtp_port}</small>{/if}
          </label>
        </div>
        <div class="grid">
          <div class="field">
            <span>IMAP security</span>
            <ThemedSelect
              value={form.imap_security}
              options={[
                { value: "tls", label: "TLS" },
                { value: "starttls", label: "STARTTLS" }
              ]}
              ariaLabel="IMAP security"
              onchange={(value) => (form.imap_security = value)}
            />
          </div>
          <div class="field">
            <span>SMTP security</span>
            <ThemedSelect
              value={form.smtp_security}
              options={[
                { value: "tls", label: "TLS" },
                { value: "starttls", label: "STARTTLS" }
              ]}
              ariaLabel="SMTP security"
              onchange={(value) => (form.smtp_security = value)}
            />
          </div>
        </div>
        {#if manualError}<p class="error" role="alert">{manualError}</p>{/if}
        <div class="form-actions">
          <button type="button" class="ghost" onclick={() => (override = "chooser")}>
            Back
          </button>
          <button type="submit" class="primary" disabled={saving}>
            {saving ? "Adding…" : "Add account"}
          </button>
        </div>
      </form>
    {/if}
  </div>
</div>

<style>
  .dialog-backdrop {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: grid;
    place-items: center;
    padding: 20px;
    background: color-mix(in srgb, #000 46%, transparent);
  }
  .dialog-panel {
    width: min(400px, 100%);
    padding: 20px;
    border: 1px solid var(--pane-border);
    border-radius: 14px;
    background: var(--bg-raised);
    box-shadow: var(--pane-highlight), var(--pane-shadow);
    color: var(--text);
  }
  .dialog-title {
    margin: 0 0 16px;
    color: var(--text-strong);
    font-size: 15px;
    font-weight: 700;
  }
  .with-action {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 6px;
  }
  .ghost.compact {
    height: 30px;
    padding: 0 8px;
    font-size: 10.5px;
    white-space: nowrap;
  }
  .manual-form {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-height: min(70vh, 610px);
    overflow-y: auto;
    padding-right: 2px;
  }
  .grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 90px;
    gap: 8px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--muted);
    font-size: 10px;
  }
  .field input {
    height: 30px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--bg);
    color: var(--text);
    font-size: 11.5px;
  }
  .field input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: 10px;
  }
  .detect-control {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .detected-check {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    flex: none;
    color: var(--success);
  }
  .form-actions {
    display: flex;
    justify-content: flex-end;
    gap: 7px;
    padding-top: 4px;
  }
  .ghost,
  .primary {
    height: 30px;
    padding: 0 11px;
    border-radius: 7px;
    font-size: 11.5px;
  }
  .ghost {
    border: 1px solid var(--border);
    color: var(--text);
  }
  .primary {
    background: var(--accent);
    color: var(--bg);
    font-weight: 650;
  }
</style>
