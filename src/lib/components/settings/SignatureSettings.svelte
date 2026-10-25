<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    Bold,
    Check,
    CircleAlert,
    Italic,
    Link2,
    RemoveFormatting,
    Save,
    Underline,
    X
  } from "lucide-svelte";
  import { accountColorValue } from "../../accountColors";
  import { api } from "../../api";
  import {
    SIGNATURE_PRESETS,
    sanitizeComposeHtml,
    sanitizeSignatureImages,
    signatureHtml,
    signatureTemplateHtml
  } from "../../compose";
  import {
    SOCIAL_NETWORKS,
    socialIconSrc,
    socialNetworkFor,
    type SocialNetwork
  } from "../../socialIcons";
  import { app } from "../../stores/app.svelte";
  import { appearance } from "../../stores/appearance.svelte";
  import ThemedSelect from "../ThemedSelect.svelte";
  import ToggleSwitch from "../ToggleSwitch.svelte";
  import type { Account, AccountInput, SignatureFields } from "../../types";
  import { errorMessage } from "./errors";

  function blankSignatureFields(): SignatureFields {
    return {
      profession: "",
      phone: "",
      location: "",
      website: "",
      socials: []
    };
  }

  let signatureAccountId = $state<number | null>(null);
  let signatureDefault = $state(false);
  let editingSpaceId = $state<number>(
    app.activeSpaceId ?? app.spaces[0]?.id ?? 1
  );
  let signatureDraft = $state("");
  let signatureFields = $state<SignatureFields>(blankSignatureFields());
  let signatureEditor = $state<HTMLDivElement | null>(null);
  let signatureSaving = $state(false);
  let signatureNotice = $state<string | null>(null);
  let signatureError = $state<string | null>(null);
  const spaceAccounts = $derived(
    app.accounts.filter((account) => account.space_id === editingSpaceId)
  );
  let signatureAccount = $derived(
    spaceAccounts.find((account) => account.id === signatureAccountId) ?? null
  );

  async function setSignatureDefault(value: boolean): Promise<void> {
    const account = signatureAccount;
    if (!account || signatureSaving) return;
    const previous = signatureDefault;
    signatureDefault = value;
    signatureNotice = null;
    signatureError = null;
    try {
      const saved = await api.updateAccount(
        account.id,
        accountInput(account, account.signature, account.signature_fields, value)
      );
      signatureDefault = saved.signature_on_new;
      await app.refresh();
    } catch (error) {
      signatureDefault = previous;
      signatureError = errorMessage(error);
    }
  }

  let signaturePreviewAccount = $derived(
    signatureAccount
      ? { ...signatureAccount, signature_fields: signatureFields }
      : null
  );
  let signaturePreview = $derived(
    signaturePreviewAccount
      ? sanitizeComposeHtml(
          sanitizeSignatureImages(signatureHtml(signatureDraft, signaturePreviewAccount))
        )
      : ""
  );

  async function selectSignatureAccount(account: Account) {
    signatureAccountId = account.id;
    signatureDefault = account.signature_on_new;
    signatureDraft = sanitizeSignatureImages(signatureTemplateHtml(account.signature));
    signatureFields = {
      ...blankSignatureFields(),
      ...account.signature_fields,
      socials: account.signature_fields.socials.map((social) => ({ ...social }))
    };
    signatureNotice = null;
    signatureError = null;
    await tick();
    if (signatureEditor) signatureEditor.innerHTML = signatureDraft;
  }

  async function openSignatures() {
    if (!signatureAccountId) {
      const first = spaceAccounts[0];
      if (first) {
        signatureAccountId = first.id;
        signatureDefault = first.signature_on_new;
        signatureDraft = sanitizeSignatureImages(signatureTemplateHtml(first.signature));
        signatureFields = {
          ...blankSignatureFields(),
          ...first.signature_fields,
          socials: first.signature_fields.socials.map((social) => ({ ...social }))
        };
      }
    }
    await tick();
    if (signatureEditor) signatureEditor.innerHTML = signatureDraft;
  }

  async function selectEditingSpace(id: number): Promise<void> {
    if (id === editingSpaceId) return;
    editingSpaceId = id;
    signatureAccountId = null;
    signatureDraft = "";
    signatureFields = blankSignatureFields();
    signatureNotice = null;
    signatureError = null;
    await tick();
    await openSignatures();
  }

  function syncSignature() {
    if (!signatureEditor) return;
    signatureDraft = signatureEditor.innerHTML;
  }

  function signatureCommand(command: string, value?: string) {
    signatureEditor?.focus();
    document.execCommand(command, false, value);
    syncSignature();
  }

  function preventSignatureBlur(event: MouseEvent) {
    event.preventDefault();
  }

  function addSignatureLink() {
    const href = window.prompt("Link URL");
    if (!href) return;
    if (!/^(https?:\/\/|mailto:)/i.test(href.trim())) {
      signatureError = "Links must start with http://, https://, or mailto:.";
      return;
    }
    signatureCommand("createLink", href.trim());
  }

  function applySignaturePreset(template: string) {
    signatureDraft = sanitizeSignatureImages(template);
    if (signatureEditor) signatureEditor.innerHTML = signatureDraft;
    signatureNotice = null;
    signatureError = null;
  }

  function hasSocialNetwork(id: string): boolean {
    return signatureFields.socials.some(
      (social) => socialNetworkFor(social.label, social.url)?.id === id
    );
  }

  function addSocialNetwork(network: SocialNetwork) {
    if (
      signatureFields.socials.length >= 6 ||
      hasSocialNetwork(network.id)
    ) {
      return;
    }
    signatureFields.socials = [
      ...signatureFields.socials,
      { label: network.label, url: network.urlPrefix }
    ];
  }

  function addSocialFromSelect(event: Event) {
    const select = event.currentTarget as HTMLSelectElement;
    const network = SOCIAL_NETWORKS.find((candidate) => candidate.id === select.value);
    select.value = "";
    if (network) addSocialNetwork(network);
  }

  function removeSocialLink(index: number) {
    signatureFields.socials = signatureFields.socials.filter(
      (_, position) => position !== index
    );
  }

  function accountInput(
    account: Account,
    signature: string,
    fields: SignatureFields,
    signatureOnNew: boolean
  ): AccountInput {
    return {
      display_name: account.display_name,
      email: account.email,
      username: account.username,
      password: "",
      imap_host: account.imap_host,
      imap_port: account.imap_port,
      imap_security: account.imap_security,
      smtp_host: account.smtp_host,
      smtp_port: account.smtp_port,
      smtp_security: account.smtp_security,
      color: account.color,
      signature,
      signature_fields: {
        profession: fields.profession.trim(),
        phone: fields.phone.trim(),
        location: fields.location.trim(),
        website: fields.website.trim(),
        socials: fields.socials
          .map((social) => ({
            label: social.label.trim(),
            url: social.url.trim()
          }))
          .filter((social) => social.label || social.url)
      },
      signature_on_new: signatureOnNew,
      space_id: account.space_id
    };
  }

  async function saveSignature() {
    const account = signatureAccount;
    if (!account) return;
    signatureSaving = true;
    signatureNotice = null;
    signatureError = null;
    try {
      const html = sanitizeComposeHtml(
        sanitizeSignatureImages(signatureEditor?.innerHTML ?? signatureDraft)
      ).trim();
      await api.updateAccount(
        account.id,
        accountInput(account, html, signatureFields, signatureDefault)
      );
      signatureDraft = html;
      if (signatureEditor) signatureEditor.innerHTML = html;
      await app.refresh();
      signatureNotice = "Signature saved.";
    } catch (error) {
      signatureError = errorMessage(error);
    } finally {
      signatureSaving = false;
    }
  }


  onMount(() => {
    void openSignatures();
  });
</script>

<div class="settings-inner">
  <div class="edit-space-row">
    <span>Edit space</span>
    <ThemedSelect
      value={editingSpaceId}
      options={app.spaces.map((space) => ({
        value: String(space.id),
        label: space.name
      }))}
      ariaLabel="Edit space"
      onchange={(value) => void selectEditingSpace(Number(value))}
    />
  </div>

  {#if spaceAccounts.length === 0}
    <p class="empty-signature">Add an account before creating a signature.</p>
  {:else}
    <div class="signature-accounts" role="group" aria-label="Signature accounts">
      {#each spaceAccounts as account (account.id)}
        <button
          type="button"
          class:active={account.id === signatureAccountId}
          aria-pressed={account.id === signatureAccountId}
          onclick={() => void selectSignatureAccount(account)}
        >
          <span class="signature-account-mark" style:background={accountColorValue(account.color, appearance.colorVision, appearance.themeDark)}>
            {account.display_name.trim().slice(0, 1).toUpperCase()}
          </span>
          <span class="signature-account-meta">
            <strong>{account.display_name}</strong>
            <small>{account.email}</small>
          </span>
          <span class="signature-status" class:on={Boolean(account.signature.trim())}></span>
        </button>
      {/each}
    </div>

    {#if signatureAccount}
      <div class="signature-default-row">
        <strong>Add signature by default</strong>
        <ToggleSwitch
          checked={signatureDefault}
          label="Add signature by default"
          onchange={(value) => void setSignatureDefault(value)}
        />
      </div>

      <div class="signature-workspace">
        <div class="signature-details">
          <div class="signature-block-head"><span>Profile</span></div>
          <div class="signature-detail-grid">
            <label class="field">
              <span>Profession</span>
              <input bind:value={signatureFields.profession} placeholder="Product designer" />
            </label>
            <label class="field">
              <span>Phone</span>
              <input bind:value={signatureFields.phone} placeholder="+39 333 123 4567" />
            </label>
            <label class="field">
              <span>Location</span>
              <input bind:value={signatureFields.location} placeholder="Milan, Italy" />
            </label>
            <label class="field">
              <span>Website</span>
              <input bind:value={signatureFields.website} placeholder="example.com" />
            </label>
          </div>

          <div class="signature-block-head social-head">
            <span>Social links</span>
            <ThemedSelect
              value=""
              placeholder="Add social…"
              options={SOCIAL_NETWORKS.filter(
                (network) => !hasSocialNetwork(network.id)
              ).map((network) => ({
                value: network.id,
                label: network.label
              }))}
              ariaLabel="Add social link"
              disabled={signatureFields.socials.length >= 6}
              onchange={(value) => {
                const network = SOCIAL_NETWORKS.find(
                  (candidate) => candidate.id === value
                );
                if (network) addSocialNetwork(network);
              }}
            />
          </div>
          {#if signatureFields.socials.length > 0}
            <div class="social-links">
              {#each signatureFields.socials as social, index (index)}
                <div class="social-link">
                  <img src={socialIconSrc(social.label, social.url)} alt="" />
                  <span class="social-name">{social.label}</span>
                  <input bind:value={social.url} placeholder="https://" aria-label={`${social.label} URL`} />
                  <button
                    class="social-remove"
                    type="button"
                    aria-label={`Remove ${social.label}`}
                    onclick={() => removeSocialLink(index)}
                  >
                    <X size={13} />
                  </button>
                </div>
              {/each}
            </div>
          {/if}
        </div>

        <div class="signature-preview-card">
          <div class="signature-block-head"><span>Preview</span></div>
          {#if signaturePreview}
            <div class="signature-preview-html">{@html signaturePreview}</div>
          {/if}
        </div>
      </div>

      <div class="signature-editor-wrap">
        <div class="signature-block-head"><span>Editor</span></div>
        <div class="signature-toolbar" role="toolbar" aria-label="Signature formatting">
          <button type="button" title="Bold" aria-label="Bold" onmousedown={preventSignatureBlur} onclick={() => signatureCommand("bold")}>
            <Bold size={14} />
          </button>
          <button type="button" title="Italic" aria-label="Italic" onmousedown={preventSignatureBlur} onclick={() => signatureCommand("italic")}>
            <Italic size={14} />
          </button>
          <button type="button" title="Underline" aria-label="Underline" onmousedown={preventSignatureBlur} onclick={() => signatureCommand("underline")}>
            <Underline size={14} />
          </button>
          <button type="button" title="Link" aria-label="Insert link" onmousedown={preventSignatureBlur} onclick={addSignatureLink}>
            <Link2 size={14} />
          </button>
          <span class="signature-toolbar-sep"></span>
          {#each ["#1f2937", "#4f46e5", "#6b7280", "#e5484d"] as color (color)}
            <button
              type="button"
              class="signature-swatch"
              style:background={color}
              aria-label={`Text color ${color}`}
              onmousedown={preventSignatureBlur}
              onclick={() => signatureCommand("foreColor", color)}
            ></button>
          {/each}
          <button type="button" title="Clear formatting" aria-label="Clear formatting" onmousedown={preventSignatureBlur} onclick={() => signatureCommand("removeFormat")}>
            <RemoveFormatting size={14} />
          </button>
        </div>
        <div
          bind:this={signatureEditor}
          class="signature-editor"
          contenteditable="true"
          role="textbox"
          tabindex="0"
          aria-multiline="true"
          aria-label="Signature editor"
          data-placeholder="Build a signature…"
          oninput={syncSignature}
        ></div>
      </div>

      <div class="signature-preset-head">
        <span>Presets</span>
      </div>
      <div class="signature-preset-grid">
        {#each SIGNATURE_PRESETS as preset (preset.id)}
          <button
            type="button"
            class:active={signatureDraft === preset.template}
            onclick={() => applySignaturePreset(preset.template)}
          >
            <span class="signature-thumb">
              <span class="signature-thumb-canvas">
                {@html sanitizeComposeHtml(signatureHtml(preset.template, signaturePreviewAccount ?? signatureAccount))}
              </span>
            </span>
            <span class="preset-title">{preset.label}</span>
          </button>
        {/each}
      </div>

      {#if signatureNotice}
        <p class="notice" role="status"><Check size={14} />{signatureNotice}</p>
      {/if}
      {#if signatureError}
        <p class="alert" role="alert"><CircleAlert size={14} />{signatureError}</p>
      {/if}

      <div class="signature-actions">
        <button
          class="ghost"
          type="button"
          disabled={signatureSaving}
          onclick={() => applySignaturePreset("")}
        >
          Clear
        </button>
        <button
          class="primary"
          type="button"
          disabled={signatureSaving}
          onclick={() => void saveSignature()}
        >
          <Save size={15} />
          {signatureSaving ? "Saving…" : "Save signature"}
        </button>
      </div>
    {/if}
  {/if}
</div>

<style>
  .edit-space-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
    color: var(--muted);
    font-size: 10.5px;
  }
  .edit-space-row :global(.themed-select) {
    width: 170px;
  }
  .signature-default-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 14px;
    padding: 10px 12px;
    border: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    border-radius: 9px;
    background: color-mix(in srgb, var(--bg-raised) 55%, var(--bg));
  }
  .signature-default-row strong {
    color: var(--text-strong);
    font-size: 11px;
    font-weight: 650;
  }
  .empty-signature {
    margin: 0;
    color: var(--muted);
    font-size: 11.5px;
  }
  .signature-accounts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
    gap: 6px;
    margin-bottom: 14px;
  }
  .signature-accounts button {
    display: flex;
    align-items: center;
    gap: 9px;
    min-width: 0;
    padding: 8px 9px;
    border: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    border-radius: 9px;
    background: color-mix(in srgb, var(--bg-raised) 62%, var(--bg));
    text-align: left;
  }
  .signature-accounts button.active {
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
    background: color-mix(in srgb, var(--accent) 9%, var(--bg-raised));
  }
  .signature-account-mark {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex: none;
    border-radius: 7px;
    color: #1f2430;
    font-size: 11px;
    font-weight: 650;
  }
  .signature-account-meta {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    min-width: 0;
  }
  .signature-account-meta strong {
    overflow: hidden;
    color: var(--text-strong);
    font-size: 10.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .signature-account-meta small {
    overflow: hidden;
    color: var(--muted);
    font-size: 9px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .signature-status {
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 50%;
    background: color-mix(in srgb, var(--border) 90%, transparent);
  }
  .signature-status.on {
    background: var(--success);
  }
  .signature-workspace {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(280px, 0.9fr);
    gap: 14px;
    align-items: start;
  }
  .signature-details,
  .signature-preview-card,
  .signature-editor-wrap {
    padding: 14px 15px;
    border: 1px solid color-mix(in srgb, var(--border) 78%, transparent);
    border-radius: 11px;
    background: color-mix(in srgb, var(--bg-raised) 70%, var(--bg));
  }
  .signature-detail-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }
  .signature-block-head,
  .signature-preset-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 9px;
    color: var(--muted);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .social-head {
    margin-top: 16px;
  }
  .social-head :global(.themed-select) {
    width: 130px;
  }
  .social-links {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .social-link {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    padding: 5px 6px;
    border-radius: 7px;
    background: color-mix(in srgb, var(--bg) 45%, transparent);
  }
  .social-link img {
    width: 14px;
    height: 14px;
    flex: none;
    object-fit: contain;
  }
  .social-name {
    width: 68px;
    flex: none;
    color: var(--text);
    font-size: 9.5px;
  }
  .social-link input {
    flex: 1;
    height: 26px;
    font-size: 10px;
  }
  .social-remove {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    flex: none;
    border-radius: 5px;
    color: var(--muted);
  }
  .social-remove:hover {
    color: var(--danger);
    background: var(--bg-hover);
  }
  .signature-preview-html :global(a),
  .signature-preview-html :global(img),
  .signature-thumb :global(a),
  .signature-thumb :global(img),
  .signature-editor :global(a),
  .signature-editor :global(img) {
    pointer-events: none;
  }
  .signature-preview-html {
    min-height: 150px;
    padding: 12px;
    border-radius: 8px;
    background: #fff;
    color: #1f2430;
    font-size: 11px;
    line-height: 1.5;
    overflow: auto;
  }
  .signature-editor-wrap {
    margin-top: 12px;
  }
  .signature-toolbar {
    display: flex;
    align-items: center;
    gap: 3px;
    flex-wrap: wrap;
    padding-bottom: 8px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .signature-toolbar button {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 6px;
    color: var(--muted);
  }
  .signature-toolbar button:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .signature-toolbar-sep {
    width: 1px;
    height: 16px;
    margin: 0 4px;
    background: color-mix(in srgb, var(--border) 60%, transparent);
  }
  .signature-swatch {
    box-shadow: inset 0 0 0 1px color-mix(in srgb, #fff 22%, transparent);
  }
  .signature-editor {
    min-height: 120px;
    margin-top: 9px;
    padding: 10px;
    border-radius: 8px;
    background: #fff;
    color: #1f2430;
    font-size: 11px;
    line-height: 1.55;
    outline: none;
    overflow: auto;
  }
  .signature-editor:empty::before {
    content: attr(data-placeholder);
    color: #8b93a3;
  }
  .signature-preset-head {
    margin-top: 18px;
  }
  .signature-preset-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
    gap: 8px;
  }
  .signature-preset-grid button {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    min-height: 176px;
    padding: 0;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
    border-radius: 10px;
    background: #fff;
    text-align: left;
  }
  .signature-preset-grid button.active {
    border-color: color-mix(in srgb, var(--accent) 65%, var(--border));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .signature-thumb {
    position: relative;
    display: block;
    flex: 1;
    width: 100%;
    min-height: 0;
    overflow: hidden;
    background: #fff;
  }
  .signature-thumb-canvas {
    position: absolute;
    top: 8px;
    left: 50%;
    width: 290px;
    padding: 10px;
    background: #fff;
    color: #1f2430;
    font-size: 11px;
    line-height: 1.5;
    transform: translateX(-50%) scale(0.58);
    transform-origin: top center;
  }
  .preset-title {
    display: block;
    padding: 7px 10px;
    border-top: 1px solid #e5e7eb;
    background: #f6f6f8;
    color: #4b5563;
    font-size: 10.5px;
    font-weight: 650;
  }
  .signature-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
  @media (max-width: 820px) {
    .signature-workspace {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
