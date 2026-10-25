<script lang="ts">
  import { Check, CircleAlert, RefreshCw } from "lucide-svelte";
  import { accountColorLabel, accountColorTokens, accountColorValue } from "../../../accountColors";
  import { api } from "../../../api";
  import { validateAccountForm, type AccountForm } from "../../../settings";
  import { activeSpaceId } from "../../../spaces";
  import { app } from "../../../stores/app.svelte";
  import { appearance } from "../../../stores/appearance.svelte";
  import type { Account, AccountInput, SignatureFields } from "../../../types";
  import { errorMessage } from "../../settings/errors";
  import OAuthSignIn from "../../account/OAuthSignIn.svelte";
  import ProviderChooser from "../../account/ProviderChooser.svelte";
  import StepHeader from "../StepHeader.svelte";

  let {
    onBack,
    onConnected,
    onSkip
  }: {
    onBack: () => void;
    onConnected: (account: Account) => void;
    onSkip: () => void;
  } = $props();

  let mode = $state<"choose" | "manual" | "google" | "microsoft">("choose");
  const defaultSpaceId = $derived(
    app.spaces.find((space) => space.name.toLowerCase() === "default")?.id ??
      activeSpaceId() ??
      1
  );

  const emptySignature = (): SignatureFields => ({
    profession: "",
    phone: "",
    location: "",
    website: "",
    socials: []
  });

  const blank = (): AccountForm => ({
    display_name: "",
    username: "",
    password: "",
    imap_host: "",
    imap_port: "993",
    imap_security: "tls",
    smtp_host: "",
    smtp_port: "465",
    smtp_security: "tls",
    color: accountColorTokens[0]
  });

  let form = $state<AccountForm>(blank());
  let errors = $state<Record<string, string>>({});
  let advanced = $state(false);
  let discovering = $state(false);
  let saving = $state(false);
  let notice = $state<string | null>(null);
  let failure = $state<string | null>(null);

  function clearError(field: keyof AccountForm) {
    if (!errors[field]) return;
    const next = { ...errors };
    delete next[field];
    errors = next;
  }

  async function discover() {
    if (!form.username.includes("@")) return;
    discovering = true;
    failure = null;
    notice = null;
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
      notice =
        found.source === "guess"
          ? "Settings guessed. Verify them before connecting."
          : "Mail settings detected.";
    } catch (error) {
      failure = errorMessage(error);
    } finally {
      discovering = false;
    }
  }

  async function submit() {
    failure = null;
    notice = null;
    if (!form.imap_host.trim() || !form.smtp_host.trim()) {
      await discover();
    }
    const found = validateAccountForm(form, true);
    errors = found;
    if (Object.keys(found).length > 0) {
      if (found.imap_host || found.smtp_host || found.imap_port || found.smtp_port) {
        advanced = true;
      }
      return;
    }
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
        signature_fields: emptySignature(),
        space_id: defaultSpaceId
      };
      const account = await api.addAccount(input);
      await app.refresh();
      void app.syncAll();
      notice = "Account added and verified.";
      onConnected(account);
    } catch (error) {
      failure = errorMessage(error);
    } finally {
      saving = false;
    }
  }

  function chooseManual() {
    mode = "manual";
  }

  function chooseGoogle() {
    mode = "google";
  }

  function chooseMicrosoft() {
    mode = "microsoft";
  }

  function chooseProvider() {
    mode = "choose";
  }

  async function onOAuthConnected(account: Account) {
    await app.refresh();
    void app.syncAll();
    notice = "Account added and verified.";
    onConnected(account);
  }
</script>

<div class="account-step">
  <StepHeader
    kicker="Account"
    title="Bring your inbox in."
    description="Connect a mailbox, or set it up later."
  />
  {#if mode === "choose"}
    <ProviderChooser
      onGoogle={chooseGoogle}
      onMicrosoft={chooseMicrosoft}
      onManual={chooseManual}
    />
    <div class="actions">
      <button class="ghost" type="button" onclick={onBack}>Back</button>
      <span class="spacer"></span>
      <button class="ghost" type="button" onclick={onSkip}>Set up later</button>
    </div>
  {:else if mode === "google" || mode === "microsoft"}
    <OAuthSignIn
      provider={mode}
      mode="add"
      spaceId={defaultSpaceId}
      onCancel={chooseProvider}
      onConnected={onOAuthConnected}
    />
    {#if notice}<p class="notice" role="status"><Check size={13} />{notice}</p>{/if}
    <div class="actions">
      <button class="ghost" type="button" onclick={onBack}>Back</button>
      <span class="spacer"></span>
      <button class="ghost" type="button" onclick={chooseProvider}>Change method</button>
    </div>
  {:else}
  <form onsubmit={(event) => { event.preventDefault(); void submit(); }}>
    <div class="form-grid">
      <label class="field">
        <span>Email address</span>
        <div class="with-action">
          <input
            type="email"
            bind:value={form.username}
            class:invalid={Boolean(errors.username)}
            aria-invalid={Boolean(errors.username)}
            autocomplete="username"
            oninput={() => clearError("username")}
            onblur={() => { if (!form.imap_host && !form.smtp_host) void discover(); }}
          />
          <button class="tiny" type="button" disabled={discovering} onclick={() => void discover()}>
            <RefreshCw size={12} />
            {discovering ? "Detecting…" : "Autodetect"}
          </button>
        </div>
        {#if errors.username}<small class="field-error">{errors.username}</small>{/if}
      </label>
      <label class="field">
        <span>Password</span>
        <input
          type="password"
          bind:value={form.password}
          class:invalid={Boolean(errors.password)}
          aria-invalid={Boolean(errors.password)}
          autocomplete="new-password"
          oninput={() => clearError("password")}
        />
        {#if errors.password}<small class="field-error">{errors.password}</small>{/if}
      </label>
      <label class="field">
        <span>Display name</span>
        <input
          bind:value={form.display_name}
          class:invalid={Boolean(errors.display_name)}
          aria-invalid={Boolean(errors.display_name)}
          autocomplete="name"
          oninput={() => clearError("display_name")}
        />
        {#if errors.display_name}<small class="field-error">{errors.display_name}</small>{/if}
      </label>
      <div class="field">
        <span>Account color</span>
        <div class="color-row">
          {#each accountColorTokens as color (color)}
            <button
              type="button"
              class:active={form.color === color}
              style:background={accountColorValue(color, appearance.colorVision, appearance.themeDark)}
              aria-label={`Use ${accountColorLabel(color)} for this account`}
              aria-pressed={form.color === color}
              onclick={() => (form.color = color)}
            ></button>
          {/each}
        </div>
      </div>
    </div>

    <details bind:open={advanced}>
      <summary>Advanced server settings</summary>
      <div class="form-grid advanced-grid">
        <label class="field">
          <span>IMAP host</span>
          <input
            bind:value={form.imap_host}
            class:invalid={Boolean(errors.imap_host)}
            aria-invalid={Boolean(errors.imap_host)}
            oninput={() => clearError("imap_host")}
          />
          {#if errors.imap_host}<small class="field-error">{errors.imap_host}</small>{/if}
        </label>
        <label class="field">
          <span>IMAP port</span>
          <input
            inputmode="numeric"
            bind:value={form.imap_port}
            class:invalid={Boolean(errors.imap_port)}
            aria-invalid={Boolean(errors.imap_port)}
            oninput={() => clearError("imap_port")}
          />
          {#if errors.imap_port}<small class="field-error">{errors.imap_port}</small>{/if}
        </label>
        <label class="field">
          <span>IMAP security</span>
          <select bind:value={form.imap_security} onchange={() => clearError("imap_security")}>
            <option value="tls">TLS</option>
            <option value="starttls">STARTTLS</option>
          </select>
        </label>
        <label class="field">
          <span>SMTP host</span>
          <input
            bind:value={form.smtp_host}
            class:invalid={Boolean(errors.smtp_host)}
            aria-invalid={Boolean(errors.smtp_host)}
            oninput={() => clearError("smtp_host")}
          />
          {#if errors.smtp_host}<small class="field-error">{errors.smtp_host}</small>{/if}
        </label>
        <label class="field">
          <span>SMTP port</span>
          <input
            inputmode="numeric"
            bind:value={form.smtp_port}
            class:invalid={Boolean(errors.smtp_port)}
            aria-invalid={Boolean(errors.smtp_port)}
            oninput={() => clearError("smtp_port")}
          />
          {#if errors.smtp_port}<small class="field-error">{errors.smtp_port}</small>{/if}
        </label>
        <label class="field">
          <span>SMTP security</span>
          <select bind:value={form.smtp_security} onchange={() => clearError("smtp_security")}>
            <option value="tls">TLS</option>
            <option value="starttls">STARTTLS</option>
          </select>
        </label>
      </div>
    </details>

    {#if notice}<p class="notice" role="status"><Check size={13} />{notice}</p>{/if}
    {#if failure}<p class="alert" role="alert"><CircleAlert size={13} />{failure}</p>{/if}
    <div class="actions">
      <button class="ghost" type="button" onclick={onBack}>Back</button>
      <button class="ghost" type="button" onclick={chooseProvider}>Change method</button>
      <span class="spacer"></span>
      <button class="ghost" type="button" disabled={saving} onclick={onSkip}>Set up later</button>
      <button class="primary" type="submit" disabled={saving}>
        {saving ? "Connecting…" : "Connect account"}
      </button>
    </div>
  </form>
  {/if}
</div>

<style>
  .account-step { display: flex; flex-direction: column; gap: 16px; }
  form { display: flex; flex-direction: column; gap: 12px; }
  .form-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
  .field { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .field > span { color: var(--muted); font-size: 9px; font-weight: 650; letter-spacing: 0.04em; }
  input,
  select {
    width: 100%;
    min-height: 32px;
    padding: 6px 9px;
    border: 1px solid color-mix(in srgb, var(--border) 78%, transparent);
    border-radius: 7px;
    background-color: color-mix(in srgb, var(--bg) 55%, var(--bg-raised));
    color: var(--text);
    font-size: 10.5px;
  }
  select {
    appearance: none;
    padding-right: 28px;
    background-image:
      linear-gradient(45deg, transparent 50%, var(--muted) 50%),
      linear-gradient(135deg, var(--muted) 50%, transparent 50%);
    background-position:
      calc(100% - 14px) 50%,
      calc(100% - 9px) 50%;
    background-size: 5px 5px, 5px 5px;
    background-repeat: no-repeat;
  }
  select option {
    background: var(--bg-raised);
    color: var(--text);
  }
  input:focus-visible,
  select:focus-visible {
    outline-offset: 1px;
  }
  input.invalid { border-color: var(--danger); }
  .with-action { display: flex; gap: 6px; }
  .with-action input { flex: 1; min-width: 0; }
  .tiny {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0 8px;
    border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    border-radius: 7px;
    color: var(--muted);
    font-size: 8.5px;
    white-space: nowrap;
  }
  .field-error { color: var(--danger); font-size: 8.5px; }
  .color-row { display: flex; align-items: center; gap: 7px; height: 32px; }
  .color-row button {
    width: 18px;
    height: 18px;
    border: 2px solid transparent;
    border-radius: 50%;
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--border) 70%, transparent);
  }
  .color-row button.active { border-color: var(--bg); box-shadow: 0 0 0 2px var(--text); }
  details {
    padding: 8px 10px;
    border: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
    border-radius: 9px;
    background: color-mix(in srgb, var(--bg-raised) 45%, transparent);
  }
  summary { color: var(--muted); font-size: 9.5px; cursor: pointer; }
  .advanced-grid { margin-top: 9px; }
  .notice,
  .alert { display: flex; align-items: center; gap: 6px; margin: 0; font-size: 9.5px; }
  .notice { color: var(--success); }
  .alert { color: var(--danger); }
  .actions { display: flex; align-items: center; gap: 8px; margin-top: 4px; }
  .spacer { flex: 1; }
  .ghost,
  .primary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 9px 15px;
    border-radius: 8px;
    font-size: 10.5px;
    font-weight: 650;
  }
  .ghost { border: 1px solid color-mix(in srgb, var(--border) 80%, transparent); color: var(--muted); }
  .primary { background: var(--accent); color: var(--bg); }
  .primary:disabled,
  .ghost:disabled { opacity: 0.55; }
  @media (max-width: 640px) { .form-grid { grid-template-columns: minmax(0, 1fr); } }
</style>
