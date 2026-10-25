<script lang="ts">
  import { Check } from "lucide-svelte";
  import { api } from "../../../api";
  import {
    applyMatugenPalette,
    applyThemeSettings,
    clearMatugenPalette,
    currentSystemPrefersDark,
    loadPreferences,
    THEME_BACKGROUNDS,
    THEME_PANES,
    updatePreferences,
    type ThemeBackground,
    type ThemePanes
  } from "../../../settings";
  import { appearance } from "../../../stores/appearance.svelte";
  import ThemeStylePreview from "../../ThemeStylePreview.svelte";
  import type { Base16 } from "../../../theme/base16";
  import { shiftPalette } from "../../../theme/colorVision";
  import {
    auraDarkTheme,
    auraLightTheme,
    defaultAccentFor,
    resolveTheme,
    themeCards,
    type ThemeCard
  } from "../../../theme/catalog";
  import StepHeader from "../StepHeader.svelte";

  let {
    onBack,
    onNext
  }: { onBack: () => void; onNext: () => void } = $props();

  const initial = loadPreferences();
  let selectedTheme = $state(initial.theme_choice);
  let background = $state<ThemeBackground>(initial.theme_background);
  let panes = $state<ThemePanes>(initial.theme_panes);
  let message = $state<string | null>(null);
  let failure = $state<string | null>(null);
  const themes = themeCards;
  const layouts = ["classic", "reader", "stacked"] as const;

  function isTauri(): boolean {
    return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  }

  function paletteFor(card: ThemeCard): Base16 {
    let palette: Base16;
    if (card.id === "light") palette = auraLightTheme;
    else if (card.id === "dark" || card.id === "system") palette = auraDarkTheme;
    else palette = resolveTheme(card.id, false, "");
    return shiftPalette(palette, appearance.colorVision);
  }

  async function selectTheme(card: ThemeCard) {
    const accent = defaultAccentFor(card.id);
    const next = updatePreferences({
      theme_choice: card.id,
      theme_accent: accent
    });
    selectedTheme = next.theme_choice;
    message = null;
    failure = null;
    clearMatugenPalette();
    if (card.id === "matugen") {
      if (isTauri()) {
        try {
          const palette = await api.reloadTheme();
          if (palette) applyMatugenPalette(palette);
          else message = "No Matugen palette found. Generate one and select it again.";
        } catch (error) {
          failure = error instanceof Error ? error.message : "Could not load the Matugen palette.";
        }
      } else {
        message = "Matugen themes are available in the desktop app.";
      }
    }
    applyThemeSettings(
      next,
      card.id === "system" ? await currentSystemPrefersDark() : undefined
    );
    if (isTauri()) {
      try {
        await api.setThemeSettings({
          choice: card.id,
          accent: accent || null,
          background: next.theme_background,
          panes: next.theme_panes
        });
      } catch {
        failure = "Theme applied locally. The desktop preference could not be saved.";
      }
    }
  }

  async function applyShape(): Promise<void> {
    failure = null;
    const next = updatePreferences({
      theme_background: background,
      theme_panes: panes
    });
    applyThemeSettings(
      next,
      next.theme_choice === "system" ? await currentSystemPrefersDark() : undefined
    );
    if (!isTauri()) return;
    try {
      await api.setThemeSettings({
        choice: next.theme_choice,
        accent: next.theme_accent || null,
        background: next.theme_background,
        panes: next.theme_panes
      });
    } catch {
      failure = "Style applied locally. The desktop preference could not be saved.";
    }
  }

  function selectBackground(value: ThemeBackground) {
    background = value;
    void applyShape();
  }

  function selectPanes(value: ThemePanes) {
    panes = value;
    void applyShape();
  }

  function layoutLabel(layout: (typeof layouts)[number]): string {
    if (layout === "reader") return "Focus";
    if (layout === "stacked") return "Stacked";
    return "Classic";
  }
</script>

<div class="appearance-step">
  <StepHeader
    kicker="Appearance"
    title="Make it yours."
    description="Pick a layout, style, and palette."
  />
  <span class="group-label">Workspace</span>
  <div class="layout-row" role="group" aria-label="Workspace layout">
    {#each layouts as layout (layout)}
      <button
        type="button"
        class:active={appearance.layoutMode === layout}
        aria-pressed={appearance.layoutMode === layout}
        onclick={() => appearance.setLayoutMode(layout)}
      >
        <span class={`layout-shape ${layout}`}><i></i><b></b><em></em></span>
        <span>{layoutLabel(layout)}</span>
      </button>
    {/each}
  </div>
  <div class="style-groups">
    <div class="style-group">
      <span class="group-label">Background</span>
      <div class="style-row" role="group" aria-label="Background">
        {#each THEME_BACKGROUNDS as option (option.id)}
          <button
            type="button"
            class:active={background === option.id}
            aria-pressed={background === option.id}
            onclick={() => selectBackground(option.id)}
          >
            <ThemeStylePreview background={option.id} {panes} compact />
            <span>{option.label}</span>
          </button>
        {/each}
      </div>
    </div>
    <div class="style-group">
      <span class="group-label">Pane dividers</span>
      <div class="style-row" role="group" aria-label="Pane dividers">
        {#each THEME_PANES as option (option.id)}
          <button
            type="button"
            class:active={panes === option.id}
            aria-pressed={panes === option.id}
            onclick={() => selectPanes(option.id)}
          >
            <ThemeStylePreview {background} panes={option.id} compact />
            <span>{option.label}</span>
          </button>
        {/each}
      </div>
    </div>
  </div>
  <span class="group-label">Theme</span>
  <div class="theme-grid" role="group" aria-label="Theme">
    {#each themes as card (card.id)}
      {@const palette = paletteFor(card)}
      <button
        type="button"
        class:active={selectedTheme === card.id}
        aria-pressed={selectedTheme === card.id}
        onclick={() => void selectTheme(card)}
      >
        <span
          class="swatch"
          style:background={card.kind === "system"
            ? `linear-gradient(135deg, ${shiftPalette(auraLightTheme, appearance.colorVision).colors.base00} 0 50%, ${shiftPalette(auraDarkTheme, appearance.colorVision).colors.base00} 50%)`
            : `linear-gradient(135deg, ${palette.colors.base00} 0 52%, ${palette.colors.base0D} 52%)`}
        >
          {#if selectedTheme === card.id}<Check size={11} />{/if}
        </span>
        <span>{card.label}</span>
      </button>
    {/each}
  </div>
  {#if message}<p class="message" role="status">{message}</p>{/if}
  {#if failure}<p class="failure" role="alert">{failure}</p>{/if}
  <div class="actions">
    <button class="ghost" type="button" onclick={onBack}>Back</button>
    <button class="primary" type="button" onclick={onNext}>Continue</button>
  </div>
</div>

<style>
  .appearance-step { display: flex; flex-direction: column; gap: 11px; }
  .group-label {
    margin-top: 3px;
    color: var(--muted);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.13em;
    text-transform: uppercase;
  }
  .theme-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 7px; }
  .theme-grid button,
  .layout-row button,
  .style-row button {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 7px;
    border: 1px solid color-mix(in srgb, var(--border) 72%, transparent);
    border-radius: 10px;
    background: color-mix(in srgb, var(--bg-raised) 58%, transparent);
    color: var(--muted);
    font-size: 9px;
    text-align: left;
  }
  .theme-grid button.active,
  .layout-row button.active,
  .style-row button.active {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
    color: var(--text-strong);
  }
  .swatch {
    display: grid;
    place-items: center;
    width: 100%;
    height: 34px;
    border-radius: 6px;
    color: #fff;
  }
  .layout-row { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 9px; }
  .layout-row button { padding: 9px; gap: 9px; }
  .style-groups { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
  .style-group { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
  .style-row { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 7px; }
  .style-row button { gap: 5px; padding: 6px; }
  .style-row button > span:last-child { font-size: 9px; }
  .layout-shape {
    display: grid;
    grid-template-columns: 22% 34% 44%;
    gap: 3px;
    width: 100%;
    height: 58px;
    overflow: hidden;
    border-radius: 7px;
    background: color-mix(in srgb, var(--bg) 70%, var(--bg-raised));
  }
  .layout-shape i,
  .layout-shape b,
  .layout-shape em {
    background: color-mix(in srgb, var(--muted) 20%, transparent);
  }
  .layout-shape.reader { grid-template-columns: 22% 0 1fr; }
  .layout-shape.reader b { overflow: hidden; opacity: 0; }
  .layout-shape.stacked { grid-template-columns: 22% 78%; grid-template-rows: 50% 50%; }
  .layout-shape.stacked i { grid-row: 1 / 3; }
  .layout-shape.stacked b { grid-column: 2; }
  .layout-shape.stacked em { grid-column: 2; }
  .layout-row button > span:last-child { font-size: 10px; }
  .message { margin: 0; color: var(--warning); font-size: 9.5px; }
  .failure { margin: 0; color: var(--danger); font-size: 9.5px; }
  .actions { display: flex; justify-content: space-between; }
  .ghost,
  .primary { padding: 9px 16px; border-radius: 8px; font-size: 10.5px; font-weight: 650; }
  .ghost { border: 1px solid color-mix(in srgb, var(--border) 80%, transparent); color: var(--muted); }
  .primary { background: var(--accent); color: var(--bg); }
  @media (max-width: 639px) {
    .theme-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); }
    .style-groups { grid-template-columns: minmax(0, 1fr); }
  }
</style>
