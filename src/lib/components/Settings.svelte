<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import {
    AtSign,
    Cog,
    Contrast,
    Palette,
    PenLine,
    SlidersHorizontal,
    Sparkles,
    X
  } from "lucide-svelte";
  import AccessibilitySettings from "./settings/AccessibilitySettings.svelte";
  import AccountSettings from "./settings/AccountSettings.svelte";
  import AiSettings from "./settings/AiSettings.svelte";
  import AppearanceSettings from "./settings/AppearanceSettings.svelte";
  import GeneralSettings from "./settings/GeneralSettings.svelte";
  import SignatureSettings from "./settings/SignatureSettings.svelte";
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
    requestNonce = 0
  }: {
    onClose: () => void;
    initialTab?: SettingsSection;
    focusAccountId?: number | null;
    requestNonce?: number;
  } = $props();

  const icons = {
    general: SlidersHorizontal,
    account: AtSign,
    appearance: Palette,
    accessibility: Contrast,
    signatures: PenLine,
    ai: Sparkles
  } as const;

  let active = $state<SettingsSection>("general");
  let activeFocusAccountId = $state<number | null>(null);
  let tabElements: (HTMLButtonElement | undefined)[] = [];
  let previousFocus: HTMLElement | null = null;

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
  });

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
        tabindex={active === tab.id ? 0 : -1}
        onclick={() => activateTab(tab.id)}
      >
        <span class="settings-tab-icon"><Icon size={15} /></span>
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
    {/if}
  </div>

  <footer class="settings-foot">
    <span><b>About AuraMail</b> · Version 0.1.0</span>
    <span>Local-first mail</span>
  </footer>
</div>
