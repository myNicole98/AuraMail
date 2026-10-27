<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import {
    Check,
    ChevronDown,
    CircleAlert,
    Palette,
    RefreshCw,
    Save,
    Trash2
  } from "lucide-svelte";
  import {
    accountColorLabel,
    accountColorTokens,
    accountColorValue,
    canonicalAccountColor,
    isAccountHexColor
  } from "../../accountColors";
  import { api } from "../../api";
  import { validateAccountForm, type AccountForm } from "../../settings";
  import ThemedSelect from "../ThemedSelect.svelte";
  import { appearance } from "../../stores/appearance.svelte";
  import type { Account, AccountInput, Space } from "../../types";

  let {
    account,
    spaces,
    onSaved,
    onRemoved,
    onColorChanged,
    onReconnect
  }: {
    account: Account;
    spaces: Space[];
    onSaved: (account: Account) => void;
    onRemoved: () => void;
    onColorChanged: (account: Account) => void;
    onReconnect: (account: Account) => void;
  } = $props();

  const SAVED_PASSWORD_MASK = "••••••••••••";
  let displayName = $state(untrack(() => account.display_name));
  let color = $state(untrack(() => canonicalAccountColor(account.color)));
  let customOpen = $state(untrack(() => isAccountHexColor(account.color)));
  let colorInput = $state<HTMLInputElement | null>(null);
  let customColor = $state(
    untrack(() =>
      /^#[0-9a-f]{6}$/i.test(account.color) ? account.color : "#a8c7fa"
    )
  );
  let spaceId = $state(untrack(() => account.space_id));
  let setupOpen = $state(false);
  let password = $state(SAVED_PASSWORD_MASK);
  let username = $state(untrack(() => account.username));
  let imapHost = $state(untrack(() => account.imap_host));
  let imapPort = $state(untrack(() => String(account.imap_port)));
  let imapSecurity = $state(untrack(() => account.imap_security));
  let smtpHost = $state(untrack(() => account.smtp_host));
  let smtpPort = $state(untrack(() => String(account.smtp_port)));
  let smtpSecurity = $state(untrack(() => account.smtp_security));
  let errors = $state<Record<string, string>>({});
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let saving = $state(false);
  let removing = $state(false);
  let reconnecting = $state(false);
  let discovering = $state(false);
  let detected = $state(false);
  let colorTimer: ReturnType<typeof setTimeout> | undefined;
  let savedTimer: ReturnType<typeof setTimeout> | undefined;
  let savedRecently = $state(false);

  const oauth = $derived(account.auth_type === "oauth");
  const proton = $derived(account.auth_type === "proton");
  const oauthProvider = $derived(
    account.oauth_provider === "microsoft" ? "microsoft" : "google"
  );
  const providerLabel = $derived(
    oauthProvider === "microsoft" ? "Microsoft" : "Google"
  );

  function input(passwordValue: string): AccountInput {
    return {
      display_name: displayName.trim(),
      email: account.email,
      username: username.trim(),
      password: passwordValue,
      imap_host: imapHost.trim(),
      imap_port: Number(imapPort),
      imap_security: imapSecurity,
      smtp_host: smtpHost.trim(),
      smtp_port: Number(smtpPort),
      smtp_security: smtpSecurity,
      color,
      signature: account.signature,
      signature_fields: account.signature_fields,
      signature_on_new: account.signature_on_new,
      space_id: spaceId
    };
  }

  onDestroy(() => {
    clearTimeout(colorTimer);
    clearTimeout(savedTimer);
  });

  async function save(): Promise<void> {
    clearTimeout(colorTimer);
    if (!oauth && !proton) {
      const form: AccountForm = {
        display_name: displayName,
        username,
        password: password === SAVED_PASSWORD_MASK ? "" : password,
        imap_host: imapHost,
        imap_port: imapPort,
        imap_security: imapSecurity,
        smtp_host: smtpHost,
        smtp_port: smtpPort,
        smtp_security: smtpSecurity,
        color
      };
      const found = validateAccountForm(form, false);
      errors = found;
      if (Object.keys(found).length > 0) {
        setupOpen = true;
        return;
      }
    }
    saving = true;
    error = null;
    notice = null;
    const moved = spaceId !== account.space_id;
    try {
      const saved = await api.updateAccount(
        account.id,
        input(oauth || proton || password === SAVED_PASSWORD_MASK ? "" : password)
      );
      if (moved && !proton) await api.moveAccount(account.id, spaceId);
      displayName = saved.display_name;
      color = saved.color;
      spaceId = saved.space_id;
      if (/^#[0-9a-f]{6}$/i.test(saved.color)) customColor = saved.color;
      savedRecently = true;
      clearTimeout(savedTimer);
      savedTimer = setTimeout(() => (savedRecently = false), 1800);
      onSaved(saved);
    } catch (caught) {
      error = caught instanceof Error ? caught.message : String(caught);
    } finally {
      saving = false;
    }
  }

  async function saveColor(next: string): Promise<void> {
    try {
      const saved = await api.setAccountColor(account.id, next);
      color = saved.color;
      onColorChanged(saved);
      error = null;
    } catch (caught) {
      color = account.color;
      error = caught instanceof Error ? caught.message : String(caught);
    }
  }

  function applyColor(next: string, immediate = false): void {
    color = next;
    clearTimeout(colorTimer);
    if (immediate) {
      void saveColor(next);
    } else {
      colorTimer = setTimeout(() => void saveColor(next), 180);
    }
  }

  async function autodetect(): Promise<void> {
    if (!username.includes("@")) {
      error = "Enter an email address first.";
      return;
    }
    discovering = true;
    detected = false;
    error = null;
    try {
      const found = await api.discoverAccountSettings(username.trim());
      imapHost = found.imap_host;
      imapPort = String(found.imap_port);
      imapSecurity = found.imap_security;
      smtpHost = found.smtp_host;
      smtpPort = String(found.smtp_port);
      smtpSecurity = found.smtp_security;
      detected = true;
    } catch (caught) {
      error = caught instanceof Error ? caught.message : String(caught);
    } finally {
      discovering = false;
    }
  }

  async function reconnect(): Promise<void> {
    reconnecting = true;
    error = null;
    notice = null;
    try {
      const saved =
        oauthProvider === "microsoft"
          ? await api.reconnectMicrosoftAccount(account.id)
          : await api.reconnectGoogleAccount(account.id);
      notice = `${providerLabel} account reconnected.`;
      onSaved(saved);
    } catch (caught) {
      error = caught instanceof Error ? caught.message : String(caught);
    } finally {
      reconnecting = false;
    }
  }

  async function remove(): Promise<void> {
    if (!window.confirm(`Remove ${account.email} and its cached mail?`)) return;
    removing = true;
    error = null;
    try {
      await api.deleteAccount(account.id);
      onRemoved();
    } catch (caught) {
      error = caught instanceof Error ? caught.message : String(caught);
    } finally {
      removing = false;
    }
  }

  function focusPassword(): void {
    if (password === SAVED_PASSWORD_MASK) password = "";
  }

  function blurPassword(): void {
    if (password === "") password = SAVED_PASSWORD_MASK;
  }
</script>

<form
  class="card form account-editor"
  onsubmit={(event) => {
    event.preventDefault();
    void save();
  }}
>
  <header class="editor-head">
    <div>
      <h4>Edit account</h4>
      <p>{account.email}</p>
    </div>
    {#if oauth}
      <span class="provider-badge">{providerLabel}</span>
    {:else if proton}
      <span class="provider-badge">
        <img src="/proton.svg" alt="" />
        Proton
      </span>
    {/if}
  </header>

  {#if notice}<p class="notice" role="status">{notice}</p>{/if}
  {#if error}<p class="alert" role="alert"><CircleAlert size={14} />{error}</p>{/if}

  <div class="grid two">
    <label class="field">
      <span>Display name</span>
      <input
        bind:value={displayName}
        class:invalid={Boolean(errors.display_name)}
        aria-invalid={Boolean(errors.display_name)}
        autocomplete="name"
      />
      {#if errors.display_name}<small class="field-error">{errors.display_name}</small>{/if}
    </label>
    <div class="field color-field">
      <span>Color</span>
      <div class="color-options" role="group" aria-label="Color">
        {#each accountColorTokens as token (token)}
          <button
            type="button"
            class:active={color === token}
            style:background={accountColorValue(
              token,
              appearance.colorVision,
              appearance.themeDark
            )}
            aria-label={accountColorLabel(token)}
            title={accountColorLabel(token)}
            onclick={() => {
              customOpen = false;
              applyColor(token, true);
            }}
          ></button>
        {/each}
        <button
          type="button"
          class="rainbow"
          class:active={isAccountHexColor(color)}
          aria-label="Custom color"
          title="Custom color"
          aria-expanded={customOpen}
          onclick={() => {
            customOpen = !customOpen;
          }}
        ></button>
      </div>
      {#if customOpen}
        <div class="custom-color" class:active={isAccountHexColor(color)}>
          <input
            class="hex-input"
            bind:value={customColor}
            aria-label="Custom color hex"
            spellcheck="false"
            maxlength="7"
            oninput={() => {
              if (/^#[0-9a-f]{6}$/i.test(customColor)) {
                applyColor(customColor.toLowerCase());
              }
            }}
          />
          <button
            type="button"
            class="picker-trigger"
            aria-label="Pick custom color"
            title="Pick custom color"
            onclick={() => {
              if (typeof colorInput?.showPicker === "function") {
                colorInput.showPicker();
              } else {
                colorInput?.click();
              }
            }}
          >
            <Palette size={14} />
          </button>
          <input
            bind:this={colorInput}
            class="native-color"
            type="color"
            bind:value={customColor}
            aria-hidden="true"
            tabindex="-1"
            oninput={() => applyColor(customColor.toLowerCase())}
          />
        </div>
      {/if}
    </div>
    <div class="field">
      <span>Space</span>
      <ThemedSelect
        value={spaceId}
        options={spaces.map((space) => ({
          value: String(space.id),
          label: space.name
        }))}
        ariaLabel="Space"
        onchange={(value) => (spaceId = Number(value))}
      />
    </div>
  </div>

  <div class="account-advanced">
    <button
      class="setup-toggle"
      type="button"
      aria-expanded={setupOpen}
      onclick={() => (setupOpen = !setupOpen)}
    >
      <span>Account setup</span>
      <ChevronDown class={setupOpen ? "open" : ""} size={14} />
    </button>
    {#if setupOpen}
      {#if oauth}
        <div class="oauth-setup">
          <button
            class="ghost"
            type="button"
            disabled={reconnecting}
            onclick={() => void reconnect()}
          >
            <RefreshCw size={15} />
            {reconnecting ? "Reconnecting…" : `Reconnect ${providerLabel}`}
          </button>
        </div>
      {:else if proton}
        <div class="oauth-setup">
          <button class="ghost" type="button" onclick={() => onReconnect(account)}>
            <RefreshCw size={15} />
            Reconnect Proton
          </button>
        </div>
      {:else}
        <div class="grid two account-fields" oninput={() => (detected = false)}>
          <label class="field">
            <span>Username</span>
            <div class="with-action">
              <input bind:value={username} autocomplete="username" />
              <div class="detect-control">
                <button
                  class="ghost compact"
                  type="button"
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
            {#if errors.username}<small class="field-error">{errors.username}</small>{/if}
          </label>
          <label class="field">
            <span>Password</span>
            <input
              type="password"
              bind:value={password}
              placeholder="Leave blank to keep the saved password"
              autocomplete="new-password"
              onfocus={focusPassword}
              onblur={blurPassword}
            />
          </label>
          <label class="field">
            <span>IMAP host</span>
            <input bind:value={imapHost} />
            {#if errors.imap_host}<small class="field-error">{errors.imap_host}</small>{/if}
          </label>
          <div class="grid two tight">
            <label class="field">
              <span>Port</span>
              <input inputmode="numeric" bind:value={imapPort} />
              {#if errors.imap_port}<small class="field-error">{errors.imap_port}</small>{/if}
            </label>
            <div class="field">
              <span>Security</span>
              <ThemedSelect
                value={imapSecurity}
                options={[
                  { value: "tls", label: "TLS" },
                  { value: "starttls", label: "STARTTLS" }
                ]}
                ariaLabel="IMAP security"
                onchange={(value) => (imapSecurity = value)}
              />
            </div>
          </div>
          <label class="field">
            <span>SMTP host</span>
            <input bind:value={smtpHost} />
            {#if errors.smtp_host}<small class="field-error">{errors.smtp_host}</small>{/if}
          </label>
          <div class="grid two tight">
            <label class="field">
              <span>Port</span>
              <input inputmode="numeric" bind:value={smtpPort} />
              {#if errors.smtp_port}<small class="field-error">{errors.smtp_port}</small>{/if}
            </label>
            <div class="field">
              <span>Security</span>
              <ThemedSelect
                value={smtpSecurity}
                options={[
                  { value: "tls", label: "TLS" },
                  { value: "starttls", label: "STARTTLS" }
                ]}
                ariaLabel="SMTP security"
                onchange={(value) => (smtpSecurity = value)}
              />
            </div>
          </div>
        </div>
      {/if}
    {/if}
  </div>

  <footer class="editor-actions">
    <button class="ghost danger" type="button" disabled={removing} onclick={() => void remove()}>
      <Trash2 size={15} />
      {removing ? "Removing…" : "Remove"}
    </button>
    <div class="save-group">
      {#if savedRecently}
        <span class="saved-check" role="status" aria-label="Saved">
          <Check size={16} />
        </span>
      {/if}
      <button class="primary" type="submit" disabled={saving}>
        <Save size={15} />
        {saving ? "Saving…" : "Save changes"}
      </button>
    </div>
  </footer>
</form>

<style>
  .account-editor {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 100%;
  }
  .editor-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
  }
  .editor-head h4 {
    margin: 0;
    color: var(--text-strong);
    font-size: 13px;
  }
  .editor-head p {
    margin: 2px 0 0;
    color: var(--muted);
    font-size: 10.5px;
  }
  .provider-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 7px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
  }
  .provider-badge img {
    width: 12px;
    height: 12px;
    border-radius: 3px;
  }
  .grid {
    display: grid;
    gap: 12px;
  }
  .grid.two {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .grid.two.tight {
    grid-template-columns: minmax(0, 1fr) 100px;
  }
  .account-fields {
    padding-top: 9px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--muted);
    font-size: 10px;
  }
  .field input {
    height: 31px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--bg);
    color: var(--text);
    font-size: 11.5px;
  }
  .field input.invalid {
    border-color: var(--danger);
  }
  .field input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .field-error {
    color: var(--danger);
    font-size: 9.5px;
  }
  .color-field {
    grid-column: 1 / -1;
  }
  .color-options {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    min-height: 28px;
    margin-top: 2px;
  }
  .color-options button {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border: 2px solid transparent;
    border-radius: 50%;
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--border) 70%, transparent);
    color: #1f2430;
  }
  .color-options button.active {
    border-color: var(--bg);
    box-shadow: 0 0 0 2px var(--text);
  }
  .color-options button.rainbow {
    background: conic-gradient(
      #f4b8c1,
      #f7c9a8,
      #f0d9a8,
      #b8e0c2,
      #a9e4e8,
      #a8c7fa,
      #c7b3f2,
      #f4b8c1
    );
  }
  .custom-color {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 7px;
  }
  .custom-color .hex-input {
    width: 92px;
    height: 28px;
  }
  .custom-color.active .hex-input {
    border-color: var(--accent);
  }
  .picker-trigger {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 1px solid var(--border);
    border-radius: 7px;
    color: var(--muted);
  }
  .picker-trigger:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .native-color {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
    pointer-events: none;
  }
  .account-advanced {
    padding-top: 8px;
  }
  .setup-toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    cursor: pointer;
    width: 100%;
    padding: 0;
    color: var(--muted);
    font-size: 10.5px;
    font-weight: 600;
    text-align: left;
  }
  .setup-toggle:hover {
    color: var(--text);
  }
  .setup-toggle :global(svg) {
    transition: transform 160ms ease;
  }
  .setup-toggle :global(svg.open) {
    transform: rotate(180deg);
  }
  .with-action {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 6px;
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
  .ghost.compact {
    height: 31px;
    padding: 0 8px;
    font-size: 10.5px;
    white-space: nowrap;
  }
  .oauth-setup {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding-top: 8px;
  }
  .editor-actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding-top: 14px;
  }
  .save-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .saved-check {
    display: grid;
    place-items: center;
    color: var(--success);
  }
  .ghost,
  .primary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 31px;
    padding: 0 11px;
    border-radius: 7px;
    font-size: 11.5px;
  }
  .ghost {
    border: 1px solid var(--border);
    color: var(--text);
  }
  .ghost.danger {
    color: var(--danger);
  }
  .primary {
    background: var(--accent);
    color: var(--bg);
    font-weight: 650;
  }
  .notice,
  .alert {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    padding: 7px 9px;
    border-radius: 7px;
    font-size: 10.5px;
  }
  .notice {
    background: color-mix(in srgb, var(--success) 10%, transparent);
    color: var(--success);
  }
  .alert {
    background: color-mix(in srgb, var(--danger) 10%, transparent);
    color: var(--danger);
  }
</style>
