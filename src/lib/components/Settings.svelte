<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import {
    AtSign,
    BrainCircuit,
    Cog,
    Contrast,
    Palette,
    PenLine,
    RefreshCw,
    SlidersHorizontal,
    Tags,
    X
  } from "lucide-svelte";
  import { listen } from "@tauri-apps/api/event";
  import { appVersion } from "../version";
  import { api } from "../api";
  import type { RuleCondition, UpdateInfo } from "../types";
  import AccessibilitySettings from "./settings/AccessibilitySettings.svelte";
  import AccountSettings from "./settings/AccountSettings.svelte";
  import AiSettings from "./settings/AiSettings.svelte";
  import AppearanceSettings from "./settings/AppearanceSettings.svelte";
  import GeneralSettings from "./settings/GeneralSettings.svelte";
  import CategoriesSettings from "./settings/CategoriesSettings.svelte";
  import RulesIcon from "./RulesIcon.svelte";
  import RulesSettings from "./settings/RulesSettings.svelte";
  import SignatureSettings from "./settings/SignatureSettings.svelte";
  import UpdateSettings from "./settings/UpdateSettings.svelte";
  import {
    SETTINGS_TABS,
    nextTabIndex,
    type SettingsSection
  } from "./settings/settingsTabs";
  import "./settings/settings.css";

  let {
    onClose,
    initialTab = "general",
    focusAccountId = null,
    requestNonce = 0,
    ruleSeed = null
  }: {
    onClose: () => void;
    initialTab?: SettingsSection;
    focusAccountId?: number | null;
    requestNonce?: number;
    ruleSeed?: { accountId?: number; conditions: RuleCondition[] } | null;
  } = $props();

  const icons = {
    general: SlidersHorizontal,
    account: AtSign,
    appearance: Palette,
    accessibility: Contrast,
    signatures: PenLine,
    ai: BrainCircuit,
    categories: Tags,
    rules: RulesIcon,
    updates: RefreshCw
  } as const;

  let active = $state<SettingsSection>("general");
  let activeFocusAccountId = $state<number | null>(null);
  let update = $state<UpdateInfo | null>(null);
  let checking = $state(false);
  let downloading = $state(false);
  let progress = $state<{ downloaded: number; total: number | null } | null>(null);
  let stagedPath = $state<string | null>(null);
  let updateError = $state<string | null>(null);

  function updateErrorMessage(error: unknown): string {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    return "Something went wrong";
  }

  function isTauri(): boolean {
    return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  }
  let tabElements: (HTMLButtonElement | undefined)[] = [];
  let previousFocus: HTMLElement | null = null;

  async function runUpdateCheck(): Promise<void> {
    if (checking || downloading) return;
    checking = true;
    try {
      const result = await api.checkUpdate();
      if (result?.version !== update?.version) {
        stagedPath = null;
        progress = null;
      }
      update = result;
      updateError = null;
    } catch {
      update = null;
    } finally {
      checking = false;
    }
  }

  async function downloadUpdate(): Promise<void> {
    if (!update?.download_url || !update.asset_name || !update.checksum) return;
    downloading = true;
    updateError = null;
    progress = { downloaded: 0, total: null };
    try {
      stagedPath = await api.downloadUpdate({
        version: update.version,
        url: update.download_url,
        assetName: update.asset_name,
        checksum: update.checksum
      });
    } catch (error) {
      updateError = updateErrorMessage(error);
      progress = null;
    } finally {
      downloading = false;
    }
  }

  async function installUpdate(): Promise<void> {
    if (!stagedPath) return;
    try {
      await api.installUpdate(stagedPath);
    } catch (error) {
      updateError = updateErrorMessage(error);
    }
  }

  $effect(() => {
    requestNonce;
    active = initialTab;
    activeFocusAccountId = focusAccountId ?? null;
  });

  function tabRef(node: HTMLButtonElement, index: number) {
    tabElements[index] = node;
    return {
      destroy() {
        tabElements[index] = undefined;
      }
    };
  }

  function selectTab(index: number, focus = false) {
    const tab = SETTINGS_TABS[index];
    if (!tab) return;
    active = tab.id;
    if (focus) {
      void tick().then(() => tabElements[index]?.focus());
    }
  }

  function activateTab(id: SettingsSection) {
    const index = SETTINGS_TABS.findIndex((tab) => tab.id === id);
    if (index >= 0) selectTab(index);
  }

  function onTabKeydown(event: KeyboardEvent, index: number) {
    const next = nextTabIndex(index, event.key, SETTINGS_TABS.length);
    if (
      next === index &&
      event.key !== "ArrowLeft" &&
      event.key !== "ArrowRight" &&
      event.key !== "Home" &&
      event.key !== "End"
    ) {
      return;
    }
    event.preventDefault();
    selectTab(next, true);
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }

  onMount(() => {
    previousFocus =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    void tick().then(() => {
      const index = SETTINGS_TABS.findIndex((tab) => tab.id === active);
      (tabElements[index] ?? tabElements[0])?.focus();
    });
    // Checked only when Settings opens; a failure stays silent.
    void runUpdateCheck();
    let unlisten: (() => void) | undefined;
    if (isTauri()) {
      void listen<{ downloaded: number; total: number | null }>(
        "update://progress",
        ({ payload }) => {
          progress = payload;
        }
      ).then((stop) => {
        unlisten = stop;
      });
    }
    return () => unlisten?.();
  });

  function openUpdate(): void {
    if (update) void api.openExternal(update.page_url).catch(() => {});
  }

  onDestroy(() => {
    previousFocus?.focus();
  });
</script>

<svelte:window onkeydown={onWindowKeydown} />

<div class="settings-root">
  <header class="settings-header">
    <span class="settings-mark" aria-hidden="true"><Cog size={20} /></span>
    <h2 class="settings-heading">Settings</h2>
    <button
      class="settings-close"
      type="button"
      aria-label="Close settings"
      onclick={onClose}
    >
      <X size={17} />
    </button>
  </header>

  <div
    class="settings-tabs"
    role="tablist"
    tabindex="-1"
    aria-label="Settings sections"
    onkeydown={(event) => {
      if (!(event.target instanceof HTMLElement)) return;
      const tab = event.target.closest<HTMLButtonElement>("[role=tab]");
      if (!tab) return;
      const index = tabElements.indexOf(tab);
      if (index >= 0) onTabKeydown(event, index);
    }}
  >
    {#each SETTINGS_TABS as tab, index (tab.id)}
      {@const Icon = icons[tab.id]}
      <button
        use:tabRef={index}
        class="settings-tab"
        class:active={active === tab.id}
        type="button"
        role="tab"
        id={`settings-tab-${tab.id}`}
        aria-selected={active === tab.id}
        aria-controls="settings-panel"
        aria-label={tab.id === "updates" && update
          ? `${tab.label} (update available)`
          : undefined}
        tabindex={active === tab.id ? 0 : -1}
        onclick={() => activateTab(tab.id)}
      >
        <span class="settings-tab-icon">
          <Icon size={15} />
          {#if tab.id === "updates" && update}
            <span class="update-dot" aria-hidden="true"></span>
          {/if}
        </span>
        <span>{tab.label}</span>
      </button>
    {/each}
  </div>

  <div
    class="settings-content"
    role="tabpanel"
    id="settings-panel"
    aria-labelledby={`settings-tab-${active}`}
    tabindex="0"
  >
    {#if active === "general"}
      <GeneralSettings />
    {:else if active === "account"}
      <AccountSettings focusAccountId={activeFocusAccountId} />
    {:else if active === "appearance"}
      <AppearanceSettings />
    {:else if active === "accessibility"}
      <AccessibilitySettings />
    {:else if active === "signatures"}
      <SignatureSettings />
    {:else if active === "ai"}
      <AiSettings />
    {:else if active === "categories"}
      <CategoriesSettings />
    {:else if active === "rules"}
      <RulesSettings {ruleSeed} {requestNonce} />
    {:else if active === "updates"}
      <UpdateSettings
        currentVersion={appVersion}
        {update}
        {checking}
        {downloading}
        {progress}
        staged={stagedPath !== null}
        error={updateError}
        onCheck={() => void runUpdateCheck()}
        onDownload={() => void downloadUpdate()}
        onInstall={() => void installUpdate()}
        onOpen={openUpdate}
      />
    {/if}
  </div>

  <footer class="settings-foot">
    <span><b>About AuraMail</b> · Version {appVersion}</span>
    <span>Local-first mail</span>
  </footer>
</div>
