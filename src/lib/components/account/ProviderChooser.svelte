<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../../api";
  import { providerIconUrls } from "../../ai/providers";

  let {
    onGoogle,
    onMicrosoft,
    onProton,
    onManual
  }: {
    onGoogle: () => void;
    onMicrosoft: () => void;
    onProton?: () => void;
    onManual: () => void;
  } = $props();

  let googleConfigured = $state<boolean | null>(null);
  let googleReason = $state<string | null>(null);
  let microsoftConfigured = $state<boolean | null>(null);
  let microsoftReason = $state<string | null>(null);

  const googleDisabled = $derived(googleConfigured !== true);
  const microsoftDisabled = $derived(microsoftConfigured !== true);
  const googleIcon = providerIconUrls("google").light;

  onMount(() => {
    void (async () => {
      try {
        const status = await api.googleOAuthStatus();
        googleConfigured = status.configured;
        googleReason = status.configured ? null : status.reason;
      } catch {
        googleConfigured = false;
        googleReason = "Google sign-in is unavailable.";
      }
    })();
    void (async () => {
      try {
        const status = await api.microsoftOAuthStatus();
        microsoftConfigured = status.configured;
        microsoftReason = status.configured ? null : status.reason;
      } catch {
        microsoftConfigured = false;
        microsoftReason = "Microsoft sign-in is unavailable.";
      }
    })();
  });
</script>

<div class="provider-chooser">
  <div class="provider-option">
    <button
      type="button"
      class="provider-button"
      disabled={googleDisabled}
      onclick={onGoogle}
    >
      <span class="provider-mark google" aria-hidden="true">
        {#if googleIcon}<img class="provider-logo" src={googleIcon} alt="" />{/if}
      </span>
      <span class="provider-name">Google</span>
    </button>
    {#if googleConfigured === false && googleReason}
      <p class="provider-reason">{googleReason}</p>
    {/if}
  </div>

  <div class="provider-option">
    <button
      type="button"
      class="provider-button"
      disabled={microsoftDisabled}
      onclick={onMicrosoft}
    >
      <span class="provider-mark microsoft" aria-hidden="true">
        <i class="ms-red"></i><i class="ms-green"></i><i class="ms-blue"></i><i class="ms-yellow"></i>
      </span>
      <span class="provider-name">Microsoft</span>
    </button>
    {#if microsoftConfigured === false && microsoftReason}
      <p class="provider-reason">{microsoftReason}</p>
    {/if}
  </div>

  {#if onProton}
    <button type="button" class="provider-button" onclick={onProton}>
      <span class="provider-mark proton" aria-hidden="true">
        <img class="provider-logo" src="/proton.svg" alt="" />
      </span>
      <span class="provider-name">Proton</span>
    </button>
  {/if}

  <button type="button" class="provider-button" onclick={onManual}>
    <span class="provider-mark" aria-hidden="true">@</span>
    <span class="provider-name">IMAP/SMTP</span>
  </button>
</div>

<style>
  .provider-chooser {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .provider-option {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .provider-button {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 13px 14px;
    border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    border-radius: 10px;
    background: color-mix(in srgb, var(--bg-raised) 70%, var(--bg));
    color: var(--text);
    text-align: left;
    transition:
      border-color 140ms ease,
      background 140ms ease;
  }
  .provider-button:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
    background: color-mix(in srgb, var(--accent) 7%, var(--bg-raised));
  }
  .provider-button:disabled {
    cursor: default;
    opacity: 0.55;
  }
  .provider-mark {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    flex: none;
    border-radius: 8px;
    background: color-mix(in srgb, var(--muted) 16%, transparent);
    color: var(--text-strong);
    font-size: 14px;
    font-weight: 700;
  }
  .provider-mark.google {
    background: #fff;
  }
  .provider-mark.proton {
    background: transparent;
  }
  .provider-logo {
    width: 18px;
    height: 18px;
    object-fit: contain;
  }
  .provider-mark.proton .provider-logo {
    width: 24px;
    height: 24px;
  }
  .provider-mark.microsoft {
    grid-template-columns: repeat(2, 8px);
    grid-template-rows: repeat(2, 8px);
    gap: 2px;
    padding: 5px;
    background: transparent;
  }
  .provider-mark.microsoft i {
    display: block;
    width: 8px;
    height: 8px;
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
  .provider-name {
    font-size: 12px;
    font-weight: 600;
  }
  .provider-reason {
    margin: 0;
    padding-left: 2px;
    color: var(--muted);
    font-size: 10px;
  }
</style>
