<script lang="ts">
  import { onMount } from "svelte";
  import { Check, CircleAlert } from "lucide-svelte";
  import { api } from "../../api";
  import {
    applyThemeSettings,
    clearMatugenPalette,
    currentSystemPrefersDark,
    defaultPreferences,
    loadPreferences,
    normalizeThemePreferences,
    reloadMatugenTheme,
    savePreferences,
    THEME_BACKGROUNDS,
    THEME_PANES,
    type LayoutMode,
    type MessagePreview,
    type SettingsPreferences,
    type ThemeBackground,
    type ThemeChoice,
    type ThemePanes
  } from "../../settings";
  import { activeSpaceId, loadSpaceAppearance, saveSpaceAppearance } from "../../spaces";
  import { app } from "../../stores/app.svelte";
  import { appearance } from "../../stores/appearance.svelte";
  import ThemedSelect from "../ThemedSelect.svelte";
  import ThemeStylePreview from "../ThemeStylePreview.svelte";
  import ToggleSwitch from "../ToggleSwitch.svelte";
  import type { Base16 } from "../../theme/base16";
  import {
    accentOptions,
    auraDarkTheme,
    auraLightTheme,
    defaultAccentFor,
    normalizeAccent,
    resolveTheme,
    themeCards,
    type ThemeCard
  } from "../../theme/catalog";
  import { errorMessage } from "./errors";
  import { shiftColor, shiftPalette } from "../../theme/colorVision";

  const layoutChoices: { value: LayoutMode; label: string }[] = [
    { value: "classic", label: "Classic" },
    { value: "reader", label: "Focus" },
    { value: "stacked", label: "Stacked" }
  ];

  const previewChoices: { value: MessagePreview; label: string }[] = [
    { value: "compact", label: "Compact" },
    { value: "preview", label: "Preview" }
  ];

  let preferences = $state<SettingsPreferences>(defaultPreferences());
  let editingSpaceId = $state<number>(activeSpaceId() ?? app.activeSpaceId ?? 1);
  let themeChoice = $state<ThemeChoice>("system");
  let themeAccent = $state("");
  let themeBackground = $state<ThemeBackground>("solid");
  let themePanes = $state<ThemePanes>("straight");
  let appearanceNotice = $state<string | null>(null);
  let appearanceError = $state<string | null>(null);
  let accentChoices = $derived(accentOptions(themeChoice));

  function isTauri(): boolean {
    return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  }

  function editingActiveSpace(): boolean {
    return editingSpaceId === (app.activeSpaceId ?? activeSpaceId());
  }

  function selectEditingSpace(id: number): void {
    editingSpaceId = id;
    const stored = loadSpaceAppearance(id);
    preferences = {
      ...preferences,
      theme_choice: stored.theme_choice,
      theme_accent: stored.theme_accent,
      theme_background: stored.theme_background,
      theme_panes: stored.theme_panes,
      layout_mode: stored.layout_mode,
      message_preview: stored.message_preview,
      dark_email: stored.dark_email
    };
    themeChoice = stored.theme_choice;
    themeAccent = stored.theme_accent;
    themeBackground = stored.theme_background;
    themePanes = stored.theme_panes;
    appearanceNotice = null;
    appearanceError = null;
  }

  function saveCurrentAppearance(): void {
    saveSpaceAppearance(editingSpaceId, {
      theme_choice: themeChoice,
      theme_accent: normalizeAccent(themeChoice, themeAccent),
      theme_background: themeBackground,
      theme_panes: themePanes,
      layout_mode: preferences.layout_mode,
      message_preview: preferences.message_preview,
      dark_email: preferences.dark_email
    });
  }

  function currentPreferences(): SettingsPreferences {
    return {
      ...preferences,
      theme_choice: themeChoice,
      theme_accent: normalizeAccent(themeChoice, themeAccent),
      theme_background: themeBackground,
      theme_panes: themePanes
    };
  }

  async function setDarkEmail(value: boolean): Promise<void> {
    preferences = { ...preferences, dark_email: value };
    await applyAppearance();
    if (editingActiveSpace()) {
      window.dispatchEvent(new CustomEvent("auramail:preferences-changed"));
    }
  }

  async function previewTheme() {
    if (!editingActiveSpace()) return;
    const current = currentPreferences();
    applyThemeSettings(
      current,
      current.theme_choice === "system" ? await currentSystemPrefersDark() : undefined
    );
  }

  function notifyPreferences() {
    savePreferences(preferences);
    void api
      .setMailPreferences({
        trusted_senders: preferences.trusted_senders,
        always_load_images: preferences.always_load_images,
        profile_pictures: preferences.profile_pictures
      })
      .catch(() => {});
    window.dispatchEvent(new CustomEvent("auramail:preferences-changed"));
  }

  function applyStoredTheme(stored: {
    choice: ThemeChoice;
    accent: string | null;
    background?: ThemeBackground;
    panes?: ThemePanes;
  }) {
    preferences = normalizeThemePreferences({
      ...preferences,
      theme_choice: stored.choice,
      theme_accent: stored.accent ?? "",
      theme_background: stored.background ?? preferences.theme_background,
      theme_panes: stored.panes ?? preferences.theme_panes
    });
    savePreferences(preferences);
    themeChoice = preferences.theme_choice;
    themeAccent = preferences.theme_accent;
    themeBackground = preferences.theme_background;
    themePanes = preferences.theme_panes;
  }

  async function loadThemeSettings() {
    if (!isTauri()) return;
    const requestedSpace = editingSpaceId;
    try {
      const stored = await api.getThemeSettings(requestedSpace);
      if (editingSpaceId !== requestedSpace) return;
      applyStoredTheme(stored);
      if (themeChoice === "matugen") {
        const result = await reloadMatugenTheme(
          true,
          () => api.reloadTheme(),
          currentPreferences()
        );
        if (result.status === "invalid") appearanceError = result.message;
      }
      const prefersDark =
        themeChoice === "system" ? await currentSystemPrefersDark() : undefined;
      applyThemeSettings(preferences, prefersDark);
    } catch {
      return;
    }
  }

  async function loadMatugen(silent: boolean): Promise<boolean> {
    if (!isTauri()) {
      appearanceError = "Matugen themes are available in the desktop app.";
      return false;
    }
    const result = await reloadMatugenTheme(
      themeChoice === "matugen",
      () => api.reloadTheme(),
      currentPreferences()
    );
    if (result.status === "disabled") {
      if (!silent) appearanceNotice = "Select Matugen to reload the theme.";
      return false;
    }
    if (result.status === "missing") {
      if (!silent) appearanceNotice = "No Matugen palette found.";
      return false;
    }
    if (result.status === "invalid") {
      appearanceError = result.message;
      return false;
    }
    appearanceError = null;
    if (!silent) appearanceNotice = "Matugen theme applied.";
    return true;
  }

  function selectTheme(choice: ThemeChoice) {
    appearanceNotice = null;
    appearanceError = null;
    themeChoice = choice;
    themeAccent = defaultAccentFor(choice);
    if (choice === "matugen" && editingActiveSpace()) {
      void loadMatugen(false);
    } else if (editingActiveSpace()) {
      clearMatugenPalette();
    }
    void previewTheme();
    void applyAppearance();
  }

  function selectAccent(accent: string) {
    appearanceNotice = null;
    appearanceError = null;
    themeAccent = normalizeAccent(themeChoice, accent);
    void previewTheme();
    void applyAppearance();
  }

  function selectBackground(background: ThemeBackground) {
    appearanceNotice = null;
    appearanceError = null;
    themeBackground = background;
    void previewTheme();
    void applyAppearance();
  }

  function selectPanes(panes: ThemePanes) {
    appearanceNotice = null;
    appearanceError = null;
    themePanes = panes;
    void previewTheme();
    void applyAppearance();
  }

  async function applyAppearance() {
    appearanceError = null;
    const next: SettingsPreferences = {
      ...preferences,
      theme_choice: themeChoice,
      theme_accent: normalizeAccent(themeChoice, themeAccent),
      theme_background: themeBackground,
      theme_panes: themePanes
    };
    preferences = next;
    saveCurrentAppearance();
    if (editingActiveSpace()) {
      savePreferences(preferences);
      if (isTauri()) {
        try {
          await api.setThemeSettings(
            {
              choice: next.theme_choice,
              accent: next.theme_accent || null,
              background: next.theme_background,
              panes: next.theme_panes
            },
            editingSpaceId
          );
        } catch (error) {
          appearanceError = errorMessage(error);
        }
      }
      const prefersDark =
        preferences.theme_choice === "system"
          ? await currentSystemPrefersDark()
          : undefined;
      applyThemeSettings(preferences, prefersDark);
    } else if (isTauri()) {
      try {
        await api.setThemeSettings(
          {
            choice: next.theme_choice,
            accent: next.theme_accent || null,
            background: next.theme_background,
            panes: next.theme_panes
          },
          editingSpaceId
        );
      } catch (error) {
        appearanceError = errorMessage(error);
      }
    }
  }

  function setLayoutMode(mode: LayoutMode) {
    preferences = { ...preferences, layout_mode: mode };
    saveCurrentAppearance();
    if (editingActiveSpace()) appearance.setLayoutMode(mode);
  }

  function setMessagePreview(mode: MessagePreview) {
    preferences = { ...preferences, message_preview: mode };
    saveCurrentAppearance();
    if (editingActiveSpace()) appearance.setMessagePreview(mode);
  }

  function cardPalette(card: ThemeCard): Base16 {
    if (card.id === "light") return shiftPalette(auraLightTheme, appearance.colorVision);
    if (card.id === "dark" || card.id === "matugen" || card.id === "system") {
      return shiftPalette(auraDarkTheme, appearance.colorVision);
    }
    const accent = card.id === themeChoice ? themeAccent : "";
    return shiftPalette(
      resolveTheme(card.id, false, accent),
      appearance.colorVision
    );
  }

  function sceneStyle(palette: Base16): string {
    return [
      `--scene-bg:${palette.colors.base00}`,
      `--scene-raised:${palette.colors.base01}`,
      `--scene-border:${palette.colors.base03}`,
      `--scene-text:${palette.colors.base05}`,
      `--scene-muted:${palette.colors.base04}`,
      `--scene-accent:${palette.colors.base0D}`
    ].join(";");
  }

  onMount(() => {
    preferences = loadPreferences();
    themeChoice = preferences.theme_choice;
    themeAccent = preferences.theme_accent;
    themeBackground = preferences.theme_background;
    themePanes = preferences.theme_panes;
    void previewTheme();
    void loadThemeSettings();
  });
</script>

{#snippet themeScene(palette: Base16)}
  <span class="theme-scene" style={sceneStyle(palette)}>
    <span class="scene-side">
      <span class="scene-brand"></span>
      <span class="scene-nav-item"></span>
      <span class="scene-nav-item"></span>
      <span class="scene-nav-item short"></span>
    </span>
    <span class="scene-content">
      <span class="scene-top">
        <span class="scene-search"></span>
        <span class="scene-avatar"></span>
      </span>
      <span class="scene-rows">
        <span class="scene-row">
          <span class="scene-dot"></span>
          <span class="scene-lines"><span></span><span></span></span>
        </span>
        <span class="scene-row">
          <span class="scene-dot"></span>
          <span class="scene-lines"><span></span><span></span></span>
        </span>
        <span class="scene-row">
          <span class="scene-dot"></span>
          <span class="scene-lines"><span></span><span></span></span>
        </span>
      </span>
      <span class="scene-accent"></span>
    </span>
  </span>
{/snippet}

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
      onchange={(value) => selectEditingSpace(Number(value))}
    />
  </div>

  {#if appearanceNotice}<p class="notice" role="status"><Check size={14} />{appearanceNotice}</p>{/if}
  {#if appearanceError}<p class="alert" role="alert"><CircleAlert size={14} />{appearanceError}</p>{/if}

  <div class="settings-label">Layout</div>
  <section class="settings-panel appearance-panel">
    <div class="layout-options" role="group" aria-label="Layout">
      {#each layoutChoices as choice (choice.value)}
        <button
          type="button"
          class="layout-option"
          class:active={preferences.layout_mode === choice.value}
          aria-pressed={preferences.layout_mode === choice.value}
          onclick={() => setLayoutMode(choice.value)}
        >
          <span class={`layout-thumb ${choice.value}`}>
            <span class="thumb-nav"></span>
            <span class="thumb-list"></span>
            <span class="thumb-reader"></span>
          </span>
          <span>{choice.label}</span>
        </button>
      {/each}
    </div>
  </section>

  <div class="settings-label">Interface</div>
  <div class="settings-bento">
    <section class="settings-panel appearance-panel mail-panel">
      <div class="settings-row">
        <strong>Use email dark mode</strong>
        <ToggleSwitch
          checked={preferences.dark_email}
          label="Use email dark mode"
          onchange={(value) => void setDarkEmail(value)}
        />
      </div>
    </section>

    <section class="settings-panel appearance-panel">
      <h4>Message list</h4>
      <div class="preview-options" role="group" aria-label="Message list">
        {#each previewChoices as choice (choice.value)}
          <button
            type="button"
            class="preview-option"
            class:active={preferences.message_preview === choice.value}
            aria-pressed={preferences.message_preview === choice.value}
            onclick={() => setMessagePreview(choice.value)}
          >
            <span class="preview-card">
              <span class="preview-card-head">
                <span class="preview-avatar">AL</span>
                <span class="preview-card-meta">
                  <span class="preview-meta-line">
                    <span class="preview-sender">Ada Lovelace</span>
                    <span class="preview-time">10:24</span>
                  </span>
                  <span class="preview-subject">Project update</span>
                </span>
              </span>
              {#if choice.value === "preview"}
                <span class="preview-snippet">
                  Here is the latest project update and what we need to do next.
                </span>
              {/if}
            </span>
            <span class="preview-label">{choice.label}</span>
          </button>
        {/each}
      </div>
    </section>
  </div>

  <div class="settings-label">Style</div>
  <section class="settings-panel appearance-panel style-panel">
    <div class="style-axis">
      <div class="style-axis-head">
        <strong>Background</strong>
      </div>
      <div class="style-options" role="group" aria-label="Background">
        {#each THEME_BACKGROUNDS as option (option.id)}
          <button
            type="button"
            class="style-card"
            class:active={themeBackground === option.id}
            aria-pressed={themeBackground === option.id}
            onclick={() => selectBackground(option.id)}
          >
            <ThemeStylePreview background={option.id} panes={themePanes} />
            <span class="theme-card-label">{option.label}</span>
          </button>
        {/each}
      </div>
    </div>

    <div class="style-axis">
      <div class="style-axis-head">
        <strong>Pane dividers</strong>
      </div>
      <div class="style-options" role="group" aria-label="Pane dividers">
        {#each THEME_PANES as option (option.id)}
          <button
            type="button"
            class="style-card"
            class:active={themePanes === option.id}
            aria-pressed={themePanes === option.id}
            onclick={() => selectPanes(option.id)}
          >
            <ThemeStylePreview background={themeBackground} panes={option.id} />
            <span class="theme-card-label">{option.label}</span>
          </button>
        {/each}
      </div>
    </div>
  </section>

  <div class="settings-label">Theme</div>
  <section class="settings-panel appearance-panel">
    <div class="theme-options" role="group" aria-label="Theme">
      {#each themeCards as card (card.id)}
        <button
          type="button"
          class="theme-option"
          class:active={themeChoice === card.id}
          aria-pressed={themeChoice === card.id}
          onclick={() => selectTheme(card.id)}
        >
          <span class="theme-thumb">
            {#if card.kind === "system"}
              <span class="system-clip light">{@render themeScene(shiftPalette(auraLightTheme, appearance.colorVision))}</span>
              <span class="system-clip dark">{@render themeScene(shiftPalette(auraDarkTheme, appearance.colorVision))}</span>
            {:else if card.kind === "matugen"}
              <span class="theme-scene matugen" style={sceneStyle(shiftPalette(auraDarkTheme, appearance.colorVision))}>
                <span class="scene-side">
                  <span class="scene-brand"></span>
                  <span class="scene-nav-item"></span>
                  <span class="scene-nav-item"></span>
                  <span class="scene-nav-item short"></span>
                </span>
                <span class="scene-content">
                  <span class="scene-top">
                    <span class="scene-search"></span>
                    <span class="scene-avatar"></span>
                  </span>
                  <span class="scene-rows">
                    <span class="scene-row">
                      <span class="scene-dot"></span>
                      <span class="scene-lines"><span></span><span></span></span>
                    </span>
                    <span class="scene-row">
                      <span class="scene-dot"></span>
                      <span class="scene-lines"><span></span><span></span></span>
                    </span>
                    <span class="scene-row">
                      <span class="scene-dot"></span>
                      <span class="scene-lines"><span></span><span></span></span>
                    </span>
                  </span>
                  <span class="scene-accent"></span>
                </span>
              </span>
            {:else}
              {@render themeScene(cardPalette(card))}
            {/if}
          </span>
          <span class="theme-card-label">{card.label}</span>
        </button>
      {/each}
    </div>

    {#if accentChoices.length > 0}
      <div class="theme-accent-row">
        <span class="theme-accent-label">Accent</span>
        <div class="accent-swatches" role="group" aria-label="Accent color">
          {#each accentChoices as accent (accent.id)}
            <button
              type="button"
              class="accent-swatch"
              class:active={themeAccent === accent.id}
              style:background={shiftColor(accent.hex, appearance.colorVision)}
              aria-label={accent.label}
              title={accent.label}
              aria-pressed={themeAccent === accent.id}
              onclick={() => selectAccent(accent.id)}
            ></button>
          {/each}
        </div>
      </div>
    {/if}

  </section>

</div>

<style>
  .edit-space-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
    color: var(--muted);
    font-size: 10.5px;
  }
  .edit-space-row :global(.themed-select) {
    width: 170px;
  }
  .appearance-panel {
    padding-bottom: 14px;
  }
  .layout-options {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
    margin-top: 10px;
  }
  .layout-option,
  .preview-option,
  .theme-option,
  .style-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    padding: 8px;
    border: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    border-radius: 9px;
    background: var(--bg);
    color: var(--muted);
    font-size: 12px;
    font-weight: 600;
    text-align: left;
  }
  .layout-option:hover,
  .preview-option:hover,
  .theme-option:hover,
  .style-card:hover {
    color: var(--text);
    border-color: color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .layout-option.active,
  .preview-option.active,
  .theme-option.active,
  .style-card.active {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
    color: var(--text);
  }
  .layout-thumb {
    display: flex;
    height: 102px;
    overflow: hidden;
    border-radius: 6px;
    background: var(--bg-raised);
  }
  .thumb-nav,
  .thumb-list,
  .thumb-reader {
    display: block;
    height: 100%;
  }
  .thumb-nav {
    width: 20%;
    flex: none;
    background: var(--bg-hover);
    border-right: 1px solid color-mix(in srgb, var(--border) 45%, transparent);
  }
  .thumb-list {
    width: 40%;
    flex: none;
    background: var(--bg);
    border-right: 1px solid color-mix(in srgb, var(--border) 45%, transparent);
  }
  .thumb-reader {
    flex: 1;
    min-width: 0;
    background: var(--bg-raised);
  }
  .layout-thumb.reader .thumb-list {
    display: none;
  }
  .layout-thumb.stacked {
    display: grid;
    grid-template-columns: 20% minmax(0, 1fr);
    grid-template-rows: 1fr 1fr;
  }
  .layout-thumb.stacked .thumb-nav {
    grid-row: 1 / 3;
    width: 100%;
  }
  .layout-thumb.stacked .thumb-list {
    width: 100%;
    grid-column: 2;
    grid-row: 1;
    border-right: 0;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 45%, transparent);
  }
  .layout-thumb.stacked .thumb-reader {
    grid-column: 2;
    grid-row: 2;
  }
  .settings-stack {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 12px;
    height: 100%;
  }
  .mail-panel {
    align-self: start;
    padding: 8px 16px 2px;
  }
  .preview-options {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 6px;
    margin-top: 10px;
  }
  .preview-card {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 7px;
    min-height: 78px;
    padding: 9px;
    border: 1px solid color-mix(in srgb, var(--border) 72%, transparent);
    border-radius: 8px;
    background: color-mix(in srgb, var(--bg-raised) 72%, var(--bg));
  }
  .preview-card-head {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  .preview-avatar {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    flex: none;
    border-radius: 50%;
    background: color-mix(in srgb, var(--accent) 18%, var(--bg-hover));
    color: var(--accent);
    font-size: 8px;
    font-weight: 700;
  }
  .preview-card-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }
  .preview-meta-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .preview-sender {
    overflow: hidden;
    flex: 1;
    color: var(--text);
    font-size: 10px;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .preview-time {
    flex: none;
    color: var(--muted);
    font-size: 8px;
  }
  .preview-subject {
    overflow: hidden;
    color: var(--text);
    font-size: 9px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .preview-snippet {
    display: -webkit-box;
    overflow: hidden;
    padding-left: 32px;
    color: var(--muted);
    font-size: 8.5px;
    line-height: 1.35;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }
  .preview-label {
    color: var(--text);
    font-size: 11px;
    font-weight: 650;
  }
  .theme-options {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
    margin-top: 10px;
  }
  .theme-option {
    gap: 7px;
    padding: 7px;
  }
  .theme-thumb {
    position: relative;
    display: block;
    height: 94px;
    overflow: hidden;
    border-radius: 6px;
    background: var(--scene-bg);
  }
  .theme-scene {
    position: absolute;
    inset: 0;
    display: flex;
    overflow: hidden;
    background: var(--scene-bg);
    color: var(--scene-text);
  }
  .scene-side {
    display: flex;
    width: 24%;
    flex: none;
    flex-direction: column;
    gap: 4px;
    padding: 7px 5px;
    background: var(--scene-raised);
    border-right: 1px solid var(--scene-border);
  }
  .scene-brand {
    width: 15px;
    height: 4px;
    margin-bottom: 3px;
    border-radius: 2px;
    background: var(--scene-accent);
  }
  .scene-nav-item {
    width: 82%;
    height: 3px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--scene-muted) 55%, transparent);
  }
  .scene-nav-item.short { width: 56%; }
  .scene-content {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
    gap: 4px;
    padding: 6px;
  }
  .scene-top {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .scene-search {
    height: 8px;
    flex: 1;
    border: 1px solid var(--scene-border);
    border-radius: 3px;
    background: var(--scene-raised);
  }
  .scene-avatar {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 50%;
    background: var(--scene-accent);
  }
  .scene-rows {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 3px;
    min-height: 0;
  }
  .scene-row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-bottom: 3px;
    border-bottom: 1px solid color-mix(in srgb, var(--scene-border) 64%, transparent);
  }
  .scene-dot {
    width: 5px;
    height: 5px;
    flex: none;
    border-radius: 50%;
    background: var(--scene-accent);
  }
  .scene-lines {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 2px;
  }
  .scene-lines span {
    height: 2px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--scene-text) 42%, transparent);
  }
  .scene-lines span:last-child {
    width: 62%;
    background: color-mix(in srgb, var(--scene-muted) 45%, transparent);
  }
  .scene-accent {
    width: 22px;
    height: 5px;
    align-self: flex-end;
    border-radius: 2px;
    background: var(--scene-accent);
  }
  .system-clip {
    position: absolute;
    inset: 0;
  }
  .system-clip.light {
    clip-path: polygon(0 0, 58% 0, 42% 100%, 0 100%);
  }
  .system-clip.dark {
    clip-path: polygon(58% 0, 100% 0, 100% 100%, 42% 100%);
  }
  .theme-scene.matugen {
    background:
      linear-gradient(135deg, #ff6b81 0%, #f4c95d 22%, #5ed69b 44%, #4dd6e8 62%, #7c8dff 82%, #d08cff 100%);
  }
  .theme-scene.matugen .scene-side {
    background: rgb(0 0 0 / 24%);
    border-color: rgb(255 255 255 / 28%);
  }
  .theme-scene.matugen .scene-search {
    border-color: rgb(255 255 255 / 30%);
    background: rgb(0 0 0 / 18%);
  }
  .theme-scene.matugen .scene-row {
    border-color: rgb(255 255 255 / 26%);
  }
  .theme-card-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .theme-accent-row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    margin-top: 12px;
  }
  .style-panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-bottom: 14px;
  }
  .style-axis {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .style-axis-head strong {
    color: var(--text);
    font-size: 11.5px;
    font-weight: 620;
  }
  .style-options {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }
  .style-card {
    gap: 7px;
    padding: 7px;
  }
  .theme-accent-label {
    color: var(--muted);
    font-size: 10px;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .accent-swatches {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .accent-swatch {
    width: 21px;
    height: 21px;
    border: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
    border-radius: 50%;
    box-shadow: inset 0 0 0 2px var(--bg);
  }
  .accent-swatch:hover {
    transform: scale(1.08);
  }
  .accent-swatch.active {
    border-color: var(--text);
    box-shadow:
      inset 0 0 0 2px var(--bg),
      0 0 0 2px color-mix(in srgb, var(--accent) 45%, transparent);
  }

</style>
