<script lang="ts">
  import {
    ArrowDownToLine,
    Check,
    RefreshCw,
    RotateCcw
  } from "lucide-svelte";
  import type { UpdateInfo } from "../../types";
  import SettingsSection from "./SettingsSection.svelte";

  let {
    currentVersion,
    update,
    checking,
    downloading,
    progress,
    staged,
    error,
    onCheck,
    onDownload,
    onInstall,
    onOpen
  }: {
    currentVersion: string;
    update: UpdateInfo | null;
    checking: boolean;
    downloading: boolean;
    progress: { downloaded: number; total: number | null } | null;
    staged: boolean;
    error: string | null;
    onCheck: () => void;
    onDownload: () => void;
    onInstall: () => void;
    onOpen: () => void;
  } = $props();

  let percent = $derived(
    progress?.total
      ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
      : null
  );
</script>

<div class="settings-inner">
<SettingsSection title="Updates">
<div class="update-body">
  {#if update}
    <span class="update-orb available" aria-hidden="true">
      <ArrowDownToLine size={24} strokeWidth={2.5} />
    </span>
  {:else}
    <span class="update-orb" aria-hidden="true">
      <Check size={28} strokeWidth={3} />
    </span>
  {/if}

  <div class="update-copy">
    <strong>
      {#if checking}
        Checking for updates…
      {:else if update && staged}
        Update ready to install
      {:else if update && downloading}
        Downloading update…{percent === null ? "" : ` ${percent}%`}
      {:else if update}
        Version {update.version} is available
      {:else}
        You're up to date
      {/if}
    </strong>
    <p>Current version: {currentVersion}</p>
  </div>

  {#if downloading}
    <div class="update-progress" aria-hidden="true">
      <span style:width={`${percent ?? 0}%`}></span>
    </div>
  {/if}

  {#if error}
    <p class="update-error" role="alert">{error}</p>
  {/if}

  <div class="update-actions">
    {#if update && staged}
      <button class="restart" type="button" onclick={onInstall}>
        <RotateCcw size={14} />
        Restart AuraMail to upgrade
      </button>
      <button class="release-link" type="button" onclick={onOpen}>
        View release
      </button>
    {:else if update && !downloading}
      {#if update.can_install}
        <button class="download" type="button" onclick={onDownload}>
          <ArrowDownToLine size={14} />
          Download update
        </button>
      {/if}
      <button class="release-link" type="button" onclick={onOpen}>
        View release
      </button>
    {:else}
      <button type="button" onclick={onCheck} disabled={checking || downloading}>
        <RefreshCw size={14} />
        Check for updates
      </button>
    {/if}
  </div>

  {#if update && !update.can_install && !downloading && !staged}
    <p class="update-note">
      Automatic installation is not available in this environment. Download the
      release from the release page.
    </p>
  {/if}
</div>
</SettingsSection>
</div>

<style>
  .update-body {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 18px;
    padding: 10px 0;
    text-align: center;
  }
  .update-orb {
    display: grid;
    place-items: center;
    width: 58px;
    height: 58px;
    border-radius: 50%;
    background: #34c759;
    color: #fff;
    box-shadow:
      0 0 0 10px color-mix(in srgb, #34c759 14%, transparent),
      0 0 28px color-mix(in srgb, #34c759 32%, transparent);
  }
  .update-orb.available {
    background: var(--accent);
    color: var(--bg);
    box-shadow:
      0 0 0 10px color-mix(in srgb, var(--accent) 12%, transparent),
      0 0 28px color-mix(in srgb, var(--accent) 28%, transparent);
  }
  .update-copy strong {
    display: block;
    color: var(--text-strong);
    font-size: 14px;
    font-weight: 600;
  }
  .update-copy p {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: 11.5px;
  }
  .update-progress {
    width: min(320px, 100%);
    height: 4px;
    overflow: hidden;
    border-radius: 999px;
    background: color-mix(in srgb, var(--border) 45%, transparent);
  }
  .update-progress span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
    transition: width 120ms linear;
  }
  .update-error {
    max-width: 420px;
    margin: 0;
    color: var(--danger);
    font-size: 11px;
  }
  .update-note {
    max-width: 420px;
    margin: 0;
    color: var(--muted);
    font-size: 11px;
    line-height: 1.5;
  }
  .update-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
  }
  .update-actions button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 12px;
    border: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
    border-radius: 8px;
    background: var(--bg-raised);
    color: var(--text);
    font-size: 11.5px;
    font-weight: 550;
  }
  .update-actions button:hover:not(:disabled) {
    background: var(--bg-hover);
  }
  .update-actions button:disabled {
    cursor: default;
    opacity: 0.55;
  }
  .update-actions .download,
  .update-actions .restart {
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }
  .update-actions .release-link {
    color: var(--muted);
  }
</style>
