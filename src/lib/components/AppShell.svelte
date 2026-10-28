<script lang="ts">
  import { onMount, tick } from "svelte";
  import { cubicOut } from "svelte/easing";
  import { fade } from "svelte/transition";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import AppRail from "./AppRail.svelte";
  import BootSplash from "./BootSplash.svelte";
  import Calendar from "./Calendar.svelte";
  import GlowBackdrop from "./GlowBackdrop.svelte";
  import MessageList from "./MessageList.svelte";
  import Reader from "./Reader.svelte";
  import Settings from "./Settings.svelte";
  import Sidebar from "./Sidebar.svelte";
  import Splitter from "./Splitter.svelte";
  import Onboarding from "./onboarding/Onboarding.svelte";
  import { api } from "../api";
  import {
    markOnboardingComplete,
    onboardingCompleted,
    shouldMigrateOnboarding,
    shouldShowOnboarding
  } from "../onboarding";
  import {
    applyMatugenPalette,
    applyThemeSettings,
    currentSystemPrefersDark,
    defaultPreferences,
    loadPreferences,
    normalizeThemePreferences,
    savePreferences,
    watchSystemTheme
  } from "../settings";
  import { app } from "../stores/app.svelte";
  import { appearance } from "../stores/appearance.svelte";
  import { desktop } from "../stores/desktop.svelte";
  import {
    defaultLayout,
    layoutColumns,
    LIST_MAX,
    LIST_MIN,
    loadLayout,
    narrowPane,
    saveLayout,
    SIDEBAR_MAX,
    SIDEBAR_MIN,
    updateSize,
    updateStackHeight,
    type LayoutState,
    type Pane
  } from "../stores/layout";

  let layout = $state<LayoutState>({ ...defaultLayout });
  let settingsOpen = $state(false);
  let onboardingDue = $state(false);
  let splashDone = $state(!isTauri());
  let splashLeaving = $state(false);
  let splashReducedMotion = $derived(
    appearance.reduceMotion || loadPreferences().reduce_motion
  );
  let openFade = $state(false);
  let width = $state(1200);
  type SettingsRequest = { tab: "general" | "account"; accountId: number | null };
  let settingsRequest = $state<SettingsRequest>({ tab: "general", accountId: null });
  let settingsRequestNonce = $state(0);
  let mode = $derived(appearance.layoutMode);
  let columns = $derived(layoutColumns(layout, width));
  let canToggle = $derived(width > 1100);
  let readerFocus = $derived(mode === "reader" && app.selectedMessageId !== null);
  let gridTemplate = $derived.by(() => {
    const rail = "48px";
    if (app.calendarOpen) {
      return `${rail} minmax(0, 1fr)`;
    }
    if (mode === "stacked") {
      return `${rail} ${columns.sidebar}px ${columns.collapsed ? 0 : 1}px minmax(0, 1fr)`;
    }
    if (readerFocus) {
      return `${rail} ${columns.sidebar}px ${columns.collapsed ? 0 : 1}px 0px 0px minmax(0, 1fr)`;
    }
    return `${rail} ${columns.sidebar}px ${columns.collapsed ? 0 : 1}px ${columns.list}px 1px minmax(0, 1fr)`;
  });
  let gridRows = $derived(
    app.calendarOpen
      ? "minmax(0, 1fr)"
      : mode === "stacked"
        ? `${layout.stackHeight}px 1px minmax(0, 1fr)`
        : "minmax(0, 1fr)"
  );
  let narrow = $derived(narrowPane(width, app.selectedMessageId !== null));
  let dragging = $state<{
    pane: Pane | "stack";
    startX: number;
    startY: number;
    startSize: number;
  } | null>(null);
  let animatingLayout = $state(false);
  let layoutTimer: ReturnType<typeof setTimeout> | undefined;
  let lastSystemDark: boolean | null = null;
  let calendarMounted = $state(false);

  $effect(() => {
    if (app.calendarOpen) calendarMounted = true;
  });

  async function applySystemTheme(force = false): Promise<void> {
    if (!app.booted || onboardingDue) return;
    const preferences = loadPreferences();
    if (preferences.theme_choice !== "system") {
      lastSystemDark = null;
      return;
    }
    const dark = await currentSystemPrefersDark();
    if (!force && dark === lastSystemDark) return;
    lastSystemDark = dark;
    applyThemeSettings(preferences, dark);
  }

  onMount(() => {
    layout = loadLayout();
    void appearance.load();
    void desktop.load();
    applyThemeSettings(loadPreferences());
    void hydrateTheme();
    void applySystemTheme(true);
    const stopTheme = watchSystemTheme(() => {
      void applySystemTheme();
    });
    let unlistenTheme: (() => void) | undefined;
    let unlistenFocus: (() => void) | undefined;
    if (isTauri()) {
      void getCurrentWindow()
        .onThemeChanged(() => {
          void applySystemTheme(true);
        })
        .then((unlisten) => {
          unlistenTheme = unlisten;
        });
      void getCurrentWindow()
        .onFocusChanged(({ payload }) => {
          document.documentElement.classList.toggle("window-inactive", !payload);
        })
        .then((unlisten) => {
          unlistenFocus = unlisten;
        });
    }
    const themePoll = setInterval(() => {
      void applySystemTheme();
    }, 2000);
    void boot();
    const onResize = () => {
      width = document.documentElement.clientWidth;
    };
    onResize();
    window.addEventListener("resize", onResize);
    const onMessageOrderChanged = () => {
      void app.reloadMessages();
    };
    window.addEventListener("auramail:message-order-changed", onMessageOrderChanged);
    const onOpenSettings = (event: Event) => {
      const detail = (event as CustomEvent<{ tab?: string; accountId?: number }>).detail;
      if (detail?.tab === "account") {
        openSettings({ tab: "account", accountId: detail.accountId ?? null });
      } else {
        openSettings();
      }
    };
    window.addEventListener("auramail:open-settings", onOpenSettings);
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener(
        "auramail:message-order-changed",
        onMessageOrderChanged
      );
      window.removeEventListener("auramail:open-settings", onOpenSettings);
      stopTheme();
      clearTimeout(layoutTimer);
      clearInterval(themePoll);
      unlistenTheme?.();
      unlistenFocus?.();
    };
  });

  function completeOnboarding(): void {
    onboardingDue = false;
    document.documentElement.style.removeProperty("--glass-backdrop-image");
    openFade = true;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        openFade = false;
      });
    });
  }

  async function boot(): Promise<void> {
    const started = Date.now();
    const reducedMotion = loadPreferences().reduce_motion;
    await app.init();
    await hydrateMailPreferences();
    const completed = onboardingCompleted();
    const migration = {
      booted: app.booted,
      accountCount: app.accounts.length,
      completed
    };
    if (shouldMigrateOnboarding(migration)) {
      markOnboardingComplete();
    }
    onboardingDue = shouldShowOnboarding({
      ...migration,
      completed: completed || shouldMigrateOnboarding(migration)
    });
    if (isTauri()) {
      const remaining = 2400 - (Date.now() - started);
      if (remaining > 0) {
        await new Promise((resolve) => setTimeout(resolve, remaining));
      }
      splashLeaving = true;
      if (!reducedMotion) {
        await new Promise((resolve) => setTimeout(resolve, 700));
      }
    }
    if (!onboardingDue) applyThemeSettings(loadPreferences());
    splashDone = true;
  }

  function isTauri(): boolean {
    return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  }

  function errorMessage(error: unknown): string {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    return "Something went wrong";
  }

  async function hydrateMailPreferences(): Promise<void> {
    if (!isTauri()) return;
    try {
      const stored = await api.getMailPreferences();
      // Re-read after the await so a concurrent theme hydration cannot be
      // overwritten with stale preferences here.
      const local = loadPreferences();
      const merged = {
        ...local,
        trusted_senders: Array.from(
          new Set([
            ...local.trusted_senders.map((sender) => sender.toLowerCase()),
            ...stored.trusted_senders.map((sender) => sender.toLowerCase())
          ])
        ),
        always_load_images:
          local.always_load_images || stored.always_load_images,
        profile_pictures:
          local.profile_pictures && stored.profile_pictures
      };
      savePreferences(merged);
      await api.setMailPreferences({
        trusted_senders: merged.trusted_senders,
        always_load_images: merged.always_load_images,
        profile_pictures: merged.profile_pictures
      });
      window.dispatchEvent(new CustomEvent("auramail:preferences-changed"));
    } catch {
      return;
    }
  }

  async function hydrateTheme(): Promise<void> {
    if (!isTauri()) return;
    try {
      const stored = await api.getThemeSettings();
      const current = loadPreferences();
      const preferences = normalizeThemePreferences({
        ...defaultPreferences(),
        ...current,
        theme_choice: stored.choice,
        theme_accent: stored.accent ?? "",
        theme_background: stored.background ?? current.theme_background,
        theme_panes: stored.panes ?? current.theme_panes
      });
      savePreferences(preferences);
      if (preferences.theme_choice === "matugen") {
        try {
          const palette = await api.reloadTheme();
          if (palette) applyMatugenPalette(palette);
        } catch (error) {
          app.setThemeNotice(errorMessage(error));
        }
      }
      const prefersDark =
        preferences.theme_choice === "system" ? await currentSystemPrefersDark() : undefined;
      applyThemeSettings(preferences, prefersDark);
    } catch {
      return;
    }
  }

  function commit(next: LayoutState) {
    layout = next;
    saveLayout(next);
  }

  function toggleSidebar() {
    if (!canToggle) return;
    commit({ ...layout, sidebarCollapsed: !layout.sidebarCollapsed });
    animatingLayout = true;
    clearTimeout(layoutTimer);
    layoutTimer = setTimeout(() => {
      animatingLayout = false;
    }, 200);
  }

  function startDrag(pane: Pane | "stack", event: PointerEvent) {
    if (!event.isPrimary || event.button !== 0) return;
    dragging = {
      pane,
      startX: event.clientX,
      startY: event.clientY,
      startSize:
        pane === "sidebar"
          ? columns.sidebar
          : pane === "stack"
            ? layout.stackHeight
            : columns.list
    };
  }

  function moveDrag(event: PointerEvent) {
    if (!dragging) return;
    if (dragging.pane === "stack") {
      commit(
        updateStackHeight(
          { ...layout, stackHeight: dragging.startSize },
          event.clientY - dragging.startY,
          window.innerHeight
        )
      );
      return;
    }
    const base =
      dragging.pane === "sidebar"
        ? { ...layout, sidebarWidth: dragging.startSize }
        : { ...layout, listWidth: dragging.startSize };
    commit(updateSize(base, dragging.pane, event.clientX - dragging.startX));
  }

  function endDrag() {
    dragging = null;
  }

  function resize(pane: Pane | "stack", delta: number) {
    if (pane === "stack") {
      commit(
        updateStackHeight(
          { ...layout, stackHeight: layout.stackHeight },
          delta,
          window.innerHeight
        )
      );
      return;
    }
    const base =
      pane === "sidebar"
        ? { ...layout, sidebarWidth: columns.sidebar }
        : { ...layout, listWidth: columns.list };
    commit(updateSize(base, pane, delta));
  }

  function openSettings(
    request: SettingsRequest = { tab: "general", accountId: null }
  ) {
    settingsRequest = request;
    settingsRequestNonce += 1;
    settingsOpen = true;
  }

  function closeSettings() {
    settingsOpen = false;
    void app.refreshAiStatus();
  }

  function settingsMotion(
    _node: Element,
    { duration = 170 }: { duration?: number } = {}
  ) {
    if (appearance.reduceMotion) return { duration: 0 };
    return {
      duration,
      css: (t: number) =>
        `transform: translate3d(0, ${(1 - t) * 8}px, 0);`
    };
  }

  function onKeydown(event: KeyboardEvent) {
    if (!app.booted || onboardingDue) return;
    const command = event.ctrlKey || event.metaKey;
    const target = event.target;
    const editing =
      target instanceof HTMLElement &&
      target.closest("input, textarea, select, [contenteditable='true']") !== null;
    if (command && event.key.toLowerCase() === "n") {
      event.preventDefault();
      app.openCompose("new");
      return;
    }
    if (command && event.key.toLowerCase() === "k") {
      event.preventDefault();
      app.toggleAuraRail();
      return;
    }
    if (event.key === "Escape") {
      if (settingsOpen) {
        event.preventDefault();
        closeSettings();
        return;
      }
      const clearMailSelection = new CustomEvent("auramail:clear-mail-selection", {
        cancelable: true
      });
      window.dispatchEvent(clearMailSelection);
      if (clearMailSelection.defaultPrevented) {
        event.preventDefault();
        return;
      }
      if (app.auraRailOpen) {
        event.preventDefault();
        app.closeAuraRail();
        return;
      }
      if (!editing && app.selectedMessageId !== null) {
        event.preventDefault();
        app.clearSelection();
        return;
      }
    }
    if (command && event.key.toLowerCase() === "b" && canToggle) {
      event.preventDefault();
      toggleSidebar();
      return;
    }
    if (editing || settingsOpen || command || event.altKey) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      void selectAdjacent(event.key === "ArrowDown" ? 1 : -1);
      return;
    }
    if (event.key === "Delete") {
      event.preventDefault();
      void deleteSelected();
    }
  }

  async function selectAdjacent(delta: number) {
    if (app.messages.length === 0) return;
    const index = app.messages.findIndex(
      (message) => message.id === app.selectedMessageId
    );
    const nextIndex =
      index < 0
        ? delta > 0
          ? 0
          : app.messages.length - 1
        : Math.min(Math.max(index + delta, 0), app.messages.length - 1);
    const id = app.messages[nextIndex]?.id;
    if (!id || id === app.selectedMessageId) return;
    await app.selectMessage(id);
    await tick();
    document
      .querySelector<HTMLElement>(`[data-message-id="${id}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }

  function focusMessageRow(id: number) {
    const row = document.querySelector<HTMLElement>(`[data-message-id="${id}"]`);
    if (!row) return;
    row.scrollIntoView?.({ block: "nearest" });
    row.querySelector<HTMLElement>(".row-open")?.focus({ preventScroll: true });
  }

  async function deleteSelected() {
    const id = app.selectedMessageId;
    if (id === null) return;
    const index = app.messages.findIndex((message) => message.id === id);
    const next = app.messages[index + 1] ?? app.messages[index - 1];
    const role = app.folders.find(
      (folder) => folder.id === app.selectedMessage?.message.folder_id
    )?.role;
    if (role === "trash" && !window.confirm("Delete this message permanently? This cannot be undone.")) {
      return;
    }
    await app.deleteMessageById(id);
    const target =
      next && app.messages.some((message) => message.id === next.id)
        ? next
        : (app.messages[index] ?? app.messages[index - 1]);
    if (!target) return;
    await app.selectMessage(target.id);
    await tick();
    focusMessageRow(target.id);
  }
</script>

<svelte:window
  onkeydown={onKeydown}
  onpointerup={endDrag}
  onpointercancel={endDrag}
/>


<GlowBackdrop force={!splashDone} exportImage={onboardingDue} />
{#if !app.booted || !splashDone}
  <div
    class="boot-layer"
    in:fade={{ duration: appearance.reduceMotion ? 0 : 500, easing: cubicOut }}
  >
    <BootSplash leaving={splashLeaving} reducedMotion={splashReducedMotion} />
  </div>
{:else if onboardingDue}
  <div class="onboarding-layer">
    <Onboarding onComplete={completeOnboarding} />
  </div>
{:else}
<div
  class="shell"
  class:animating={animatingLayout}
  class:narrow={narrow !== null}
  class:reader-open={narrow === "reader"}
  class:stacked={mode === "stacked" && !app.calendarOpen}
  class:reader-focus={readerFocus}
  class:calendar-open={app.calendarOpen}
  style:grid-template-columns={gridTemplate}
  style:grid-template-rows={gridRows}
>
  <AppRail collapsed={columns.collapsed} onOpenSettings={() => openSettings()} />
  {#if calendarMounted}
    <div class="calendar-host" class:hidden={!app.calendarOpen}>
      <Calendar />
    </div>
  {/if}
  {#if !app.calendarOpen}
    <Sidebar
      collapsed={columns.collapsed}
      canToggle={canToggle}
      onToggle={toggleSidebar}
    />
    <div class="sidebar-splitter" class:hidden={columns.collapsed} inert={columns.collapsed} aria-hidden={columns.collapsed}>
      <Splitter
        label="Resize sidebar"
        value={columns.sidebar}
        min={SIDEBAR_MIN}
        max={SIDEBAR_MAX}
        onPointerDown={(event) => startDrag("sidebar", event)}
        onKeyboardResize={(delta) => resize("sidebar", delta)}
      />
    </div>
    <MessageList />
    <div class="list-splitter">
      <Splitter
        label={mode === "stacked" ? "Resize stacked panes" : "Resize message list"}
        value={mode === "stacked" ? layout.stackHeight : columns.list}
        min={mode === "stacked" ? 140 : LIST_MIN}
        max={mode === "stacked" ? 900 : LIST_MAX}
        orientation={mode === "stacked" ? "horizontal" : "vertical"}
        onPointerDown={(event) =>
          startDrag(mode === "stacked" ? "stack" : "list", event)}
        onKeyboardResize={(delta) =>
          resize(mode === "stacked" ? "stack" : "list", delta)}
      />
    </div>
    <Reader />
  {/if}
</div>

<div
  class="space-fade"
  class:active={app.spaceSwitching && !appearance.reduceMotion}
  aria-hidden="true"
></div>

{#if dragging}
  <div
    class="drag-overlay"
    class:row-resize={dragging.pane === "stack"}
    role="presentation"
    onpointermove={moveDrag}
    onpointerup={endDrag}
    onpointercancel={endDrag}
  ></div>
{/if}

{#if settingsOpen}
  <div class="settings-layer" transition:settingsMotion>
    <Settings
      onClose={closeSettings}
      initialTab={settingsRequest.tab}
      focusAccountId={settingsRequest.accountId}
      requestNonce={settingsRequestNonce}
    />
  </div>
{/if}

{#if app.themeNotice}
  <div class="theme-toast" role="status">
    <span>{app.themeNotice}</span>
    <button
      type="button"
      aria-label="Dismiss theme notice"
      onclick={() => app.setThemeNotice(null)}
    >
      ×
    </button>
  </div>
{/if}
{/if}

<div class="open-fade" class:active={openFade} aria-hidden="true"></div>

<style>
  .shell {
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    height: 100vh;
    height: 100dvh;
    overflow: hidden;
    background: var(--bg);
  }
  .calendar-host {
    grid-column: 2;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
    height: 100%;
  }
  .calendar-host.hidden {
    display: none;
  }
  .calendar-host :global(.calendar-workspace) {
    grid-column: 1;
    width: 100%;
    height: 100%;
  }
  .open-fade {
    position: fixed;
    z-index: 45;
    inset: 0;
    background: var(--bg);
    opacity: 0;
    pointer-events: none;
    transition: opacity 550ms cubic-bezier(0.22, 0.8, 0.2, 1);
  }
  .open-fade.active {
    opacity: 1;
    transition: none;
  }
  .space-fade {
    position: fixed;
    z-index: 50;
    inset: 0;
    background: var(--bg);
    opacity: 0;
    pointer-events: none;
    transition: opacity 180ms ease;
  }
  .space-fade.active {
    opacity: 1;
  }
  .shell.animating {
    transition: grid-template-columns 180ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .sidebar-splitter {
    min-width: 0;
    /* Splitter keeps its 1px visual line, but its ::after hit target extends
       past this wrapper. Do not clip it back to 1px. */
    overflow: visible;
  }
  .list-splitter {
    min-width: 0;
    min-height: 0;
  }
  .sidebar-splitter.hidden {
    pointer-events: none;
  }
  .drag-overlay {
    position: fixed;
    inset: 0;
    z-index: 200;
    cursor: col-resize;
    touch-action: none;
  }
  .drag-overlay.row-resize {
    cursor: row-resize;
  }
  .theme-toast {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 80;
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: min(420px, calc(100vw - 32px));
    padding: 10px 12px;
    border: 1px solid color-mix(in srgb, var(--warning) 45%, transparent);
    border-radius: 9px;
    background: var(--bg-raised);
    box-shadow: 0 8px 24px color-mix(in srgb, var(--border) 45%, transparent);
    color: var(--text);
    font-size: 11.5px;
    line-height: 1.4;
  }
  .theme-toast span {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .theme-toast button {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    flex: none;
    border-radius: 5px;
    color: var(--muted);
    font-size: 15px;
  }
  .theme-toast button:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .shell.stacked :global(.sidebar) {
    grid-column: 2;
    grid-row: 1 / 4;
  }
  .shell.stacked .sidebar-splitter {
    grid-column: 3;
    grid-row: 1 / 4;
  }
  .shell.stacked :global(.message-list) {
    grid-column: 4;
    grid-row: 1;
  }
  .shell.stacked .list-splitter {
    grid-column: 4;
    grid-row: 2;
    height: 1px;
  }
  .shell.stacked :global(.reader) {
    grid-column: 4;
    grid-row: 3;
  }
  .shell.reader-focus :global(.message-list),
  .shell.reader-focus .list-splitter {
    display: none;
  }
  .shell.reader-focus :global(.reader) {
    grid-column: 6;
  }
  .shell.narrow {
    grid-template-columns: 1fr !important;
    grid-template-rows: minmax(0, 1fr) !important;
  }
  .shell.narrow :global(.app-rail),
  .shell.narrow :global(.sidebar),
  .shell.narrow .sidebar-splitter,
  .shell.narrow .list-splitter,
  .shell.narrow :global(.splitter) {
    display: none;
  }
  .shell.narrow :global(.calendar-workspace) {
    grid-column: 1;
    grid-row: 1;
  }
  .shell.narrow :global(.message-list),
  .shell.narrow :global(.reader) {
    grid-column: 1;
    grid-row: 1;
  }
  .shell.narrow :global(.reader) {
    display: none;
  }
  .shell.narrow.reader-open :global(.message-list) {
    display: none;
  }
  .shell.narrow.reader-open :global(.reader) {
    display: flex;
  }
  .settings-layer {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: var(--bg);
    will-change: transform;
  }
</style>
