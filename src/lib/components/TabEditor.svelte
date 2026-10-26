<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { CircleAlert, Search, Trash2, X } from "lucide-svelte";
  import { app } from "../stores/app.svelte";
  import { appearance } from "../stores/appearance.svelte";
  import type { Tab } from "../types";
  import {
    TAB_COLORS,
    isHexColor,
    tabColor,
    tabIconSearch,
    tabIcons
  } from "../tabAppearance";

  const PANEL_WIDTH = 372;

  let {
    tab = null,
    anchor,
    defaultColor = TAB_COLORS[0],
    onClose
  }: {
    tab?: Tab | null;
    anchor: { x: number; y: number };
    defaultColor?: string;
    onClose: () => void;
  } = $props();

  let name = $state(untrack(() => tab?.name ?? ""));
  let description = $state(untrack(() => tab?.description ?? ""));
  let color = $state(
    untrack(() =>
      tab?.color && isHexColor(tab.color) ? tab.color : defaultColor
    )
  );
  let icon = $state(
    untrack(() =>
      tab?.icon && tabIcons.some((option) => option.id === tab.icon)
        ? tab.icon
        : "folder"
    )
  );
  let iconQuery = $state("");
  let saving = $state(false);
  let error = $state<string | null>(null);
  let editor = $state<HTMLDivElement | null>(null);
  let nameInput = $state<HTMLInputElement | null>(null);
  let descriptionInput = $state<HTMLTextAreaElement | null>(null);

  let icons = $derived(tabIconSearch(iconQuery));
  let isCustomColor = $derived(
    !TAB_COLORS.some((swatch) => swatch.toLowerCase() === color.toLowerCase())
  );
  let PreviewIcon = $derived(
    tabIcons.find((option) => option.id === icon)?.component ??
      tabIcons[0].component
  );
  let left = $derived(
    Math.max(
      12,
      Math.min(anchor.x, (window?.innerWidth ?? 1280) - PANEL_WIDTH - 12)
    )
  );
  let top = $derived(anchor.y + 8);

  onMount(() => {
    void tick().then(() => {
      nameInput?.focus();
      nameInput?.select();
    });
  });

  function previewColor(value: string, index = 0): string {
    return tabColor(
      { color: value } as Tab,
      index,
      appearance.colorVision,
      appearance.themeDark
    );
  }

  function sameColor(a: string, b: string): boolean {
    return a.toLowerCase() === b.toLowerCase();
  }

  function outside(event: PointerEvent) {
    const target = event.target;
    if (!(target instanceof Node) || !editor) return;
    if (!editor.contains(target)) onClose();
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    onClose();
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (saving) return;
    const trimmedName = name.trim();
    const trimmedDescription = description.trim();
    if (!trimmedName) {
      error = "Give this tab a name.";
      nameInput?.focus();
      return;
    }
    if (!trimmedDescription) {
      error = "Describe which mail belongs here.";
      descriptionInput?.focus();
      return;
    }
    saving = true;
    error = null;
    const result = tab
      ? await app.updateTab(tab.id, trimmedName, trimmedDescription, color, icon)
      : await app.createTab(trimmedName, trimmedDescription, color, icon);
    saving = false;
    if (result) {
      error = result;
      return;
    }
    onClose();
  }

  async function remove() {
    if (!tab || saving) return;
    if (!window.confirm(`Delete the "${tab.name}" tab?`)) return;
    saving = true;
    error = null;
    const result = await app.deleteTab(tab.id);
    saving = false;
    if (result) {
      error = result;
      return;
    }
    onClose();
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={onKeydown} />

<div
  class="editor"
  bind:this={editor}
  role="dialog"
  aria-labelledby="tab-editor-title"
  style:left={`${left}px`}
  style:top={`${top}px`}
  style:max-height={`calc(100vh - ${top + 12}px)`}
  style:--tab-color={previewColor(color)}
>
  <form onsubmit={save}>
    <header class="editor-head">
      <span class="tab-preview" aria-hidden="true">
        <PreviewIcon size={15} />
      </span>
      <div class="editor-title">
        <strong id="tab-editor-title">{tab ? "Edit tab" : "New tab"}</strong>
        <span>{tab ? tab.name : "Group related mail"}</span>
      </div>
      <button
        type="button"
        class="close"
        aria-label="Close"
        title="Close"
        onclick={onClose}
      >
        <X size={15} />
      </button>
    </header>

    <div class="editor-body">
      <div class="field">
        <label class="field-label" for="tab-name">Name</label>
        <input
          id="tab-name"
          class="name-input"
          bind:this={nameInput}
          bind:value={name}
          maxlength="60"
          autocomplete="off"
          spellcheck="false"
          oninput={() => (error = null)}
        />
      </div>

      <div class="field">
        <label class="field-label" for="tab-description">Description</label>
        <textarea
          id="tab-description"
          class="description"
          rows="3"
          bind:this={descriptionInput}
          bind:value={description}
          maxlength="500"
          placeholder="Describe which mail belongs here"
          oninput={() => (error = null)}
        ></textarea>
      </div>

      <div class="field">
        <div class="field-head">
          <span class="field-label">Color</span>
          <span class="color-value">{color.toUpperCase()}</span>
        </div>
        <div class="swatches">
          {#each TAB_COLORS as swatch, index (swatch)}
            <button
              type="button"
              class="swatch"
              class:active={sameColor(color, swatch)}
              style:--c={previewColor(swatch, index)}
              aria-label={`Use color ${swatch}`}
              aria-pressed={sameColor(color, swatch)}
              title={swatch}
              onclick={() => (color = swatch)}
            ></button>
          {/each}
          <label
            class="custom-color"
            class:active={isCustomColor}
            style:--c={previewColor(color)}
            title="Choose a custom color"
          >
            <input
              type="color"
              bind:value={color}
              aria-label="Custom tab color"
            />
          </label>
        </div>
      </div>

      <div class="field">
        <span class="field-label">Icon</span>
        <label class="icon-search">
          <Search size={14} aria-hidden="true" />
          <input
            bind:value={iconQuery}
            aria-label="Search tab icons"
            placeholder="Search icons"
            autocomplete="off"
            spellcheck="false"
            onkeydown={(event) => {
              if (event.key === "Enter") event.preventDefault();
            }}
          />
        </label>
        <div class="icon-grid" role="group" aria-label="Tab icon">
          {#each icons as option (option.id)}
            {@const Icon = option.component}
            <button
              type="button"
              class="icon-button"
              class:active={icon === option.id}
              aria-label={option.label}
              aria-pressed={icon === option.id}
              title={option.label}
              onclick={() => (icon = option.id)}
            >
              <Icon size={16} />
            </button>
          {/each}
          {#if icons.length === 0}
            <span class="no-icons">No matching icons</span>
          {/if}
        </div>
      </div>

    </div>

    {#if error}
      <p class="error" role="alert">
        <CircleAlert size={13} aria-hidden="true" />
        <span>{error}</span>
      </p>
    {/if}

    <div class="actions">
      {#if tab}
        <button type="button" class="delete" disabled={saving} onclick={remove}>
          <Trash2 size={14} />
          Delete tab
        </button>
      {/if}
      <span class="spacer"></span>
      <button type="button" class="cancel" disabled={saving} onclick={onClose}>
        Cancel
      </button>
      <button
        type="submit"
        class="save"
        disabled={saving}
        aria-busy={saving}
      >
        {saving ? "Saving…" : tab ? "Save" : "Create tab"}
      </button>
    </div>
  </form>
</div>

<style>
  .editor {
    position: fixed;
    z-index: 120;
    display: flex;
    flex-direction: column;
    width: min(372px, calc(100vw - 24px));
    padding: 16px;
    overflow: hidden;
    border: 1px solid var(--pane-border);
    border-radius: 15px;
    background: var(--bg-raised);
    box-shadow:
      var(--pane-highlight),
      0 24px 60px color-mix(in srgb, #000 30%, transparent);
    color: var(--text);
    animation: editor-in 140ms ease-out both;
  }
  :global(html[data-theme-glass="true"]) .editor {
    background: var(--glass-raised);
    border-color: var(--glass-border);
    -webkit-backdrop-filter: blur(24px) saturate(var(--glass-saturate));
    backdrop-filter: blur(24px) saturate(var(--glass-saturate));
  }
  form {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .editor-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 14px;
  }
  .tab-preview {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    flex: none;
    border-radius: 9px;
    background: color-mix(in srgb, var(--tab-color) 14%, transparent);
    color: var(--tab-color);
    box-shadow: inset 0 0 0 1px
      color-mix(in srgb, var(--tab-color) 30%, transparent);
  }
  .editor-title {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }
  .editor-title strong {
    color: var(--text-strong);
    font-size: 13px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  .editor-title span {
    overflow: hidden;
    color: var(--muted);
    font-size: 9.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .close {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex: none;
    border-radius: 7px;
    color: var(--muted);
  }
  .close:hover,
  .close:focus-visible {
    background: var(--bg-hover);
    color: var(--text);
  }
  .editor-body {
    display: flex;
    flex-direction: column;
    gap: 13px;
    min-height: 0;
    margin-right: -4px;
    padding-right: 4px;
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-width: thin;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field-label {
    color: var(--muted);
    font-size: 9.5px;
    font-weight: 650;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }
  .field-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }
  .color-value {
    color: var(--muted);
    font-family: var(--font-mono);
    font-size: 9px;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.03em;
  }
  .name-input,
  .description {
    width: 100%;
    border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    border-radius: 8px;
    background: color-mix(in srgb, var(--bg) 62%, transparent);
    color: var(--text-strong);
    font: inherit;
  }
  .name-input {
    height: 34px;
    padding: 0 10px;
    font-size: 13px;
    font-weight: 600;
  }
  .description {
    min-height: 68px;
    padding: 9px 10px;
    color: var(--text);
    font-size: 11.5px;
    line-height: 1.45;
    resize: vertical;
  }
  .name-input::placeholder,
  .description::placeholder {
    color: color-mix(in srgb, var(--muted) 78%, transparent);
  }
  .name-input:focus-visible,
  .description:focus-visible {
    outline: 0;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .swatches {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 7px;
    min-height: 28px;
  }
  .swatch {
    position: relative;
    width: 21px;
    height: 21px;
    flex: none;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--c);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, #000 12%, transparent);
    transition:
      transform 120ms ease,
      box-shadow 120ms ease;
  }
  .swatch:hover {
    transform: scale(1.08);
  }
  .swatch.active {
    box-shadow:
      0 0 0 2px var(--bg-raised),
      0 0 0 3.5px var(--c);
  }
  .swatch:focus-visible {
    box-shadow:
      0 0 0 2px var(--bg-raised),
      0 0 0 4px var(--accent);
  }
  .custom-color {
    position: relative;
    display: block;
    width: 21px;
    height: 21px;
    flex: none;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background-color: var(--muted);
    background-image: conic-gradient(
      from 30deg,
      #e6797e,
      #f2b85b,
      #58c98b,
      #6da4ff,
      #b489ff,
      #e6797e
    );
    box-shadow: inset 0 0 0 1px color-mix(in srgb, #000 12%, transparent);
    cursor: pointer;
    transition:
      transform 120ms ease,
      box-shadow 120ms ease;
  }
  .custom-color:hover {
    transform: scale(1.08);
  }
  .custom-color.active {
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, #000 12%, transparent),
      0 0 0 2px var(--bg-raised),
      0 0 0 3.5px var(--c);
  }
  .custom-color:focus-within {
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, #000 12%, transparent),
      0 0 0 2px var(--bg-raised),
      0 0 0 4px var(--accent);
  }
  .custom-color input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
  }
  .icon-search {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 9px;
    border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    border-radius: 8px;
    background: color-mix(in srgb, var(--bg) 62%, transparent);
    color: var(--muted);
    transition:
      border-color 120ms ease,
      box-shadow 120ms ease;
  }
  .icon-search:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .icon-search input {
    width: 100%;
    height: 100%;
    border: 0;
    outline: 0;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 11px;
  }
  .icon-search input::placeholder {
    color: var(--muted);
  }
  .icon-grid {
    display: grid;
    grid-template-columns: repeat(8, minmax(0, 1fr));
    gap: 4px;
    max-height: 144px;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding-right: 4px;
    scrollbar-width: thin;
  }
  .icon-button {
    display: grid;
    place-items: center;
    height: 32px;
    border-radius: 7px;
    color: var(--muted);
    transition:
      background-color 120ms ease,
      color 120ms ease,
      box-shadow 120ms ease;
  }
  .icon-button:hover,
  .icon-button:focus-visible {
    background: var(--bg-hover);
    color: var(--text);
  }
  .icon-button.active {
    background: color-mix(in srgb, var(--tab-color) 14%, transparent);
    color: var(--tab-color);
    box-shadow: inset 0 0 0 1px
      color-mix(in srgb, var(--tab-color) 40%, transparent);
  }
  .icon-button:focus-visible {
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 55%, transparent);
  }
  .no-icons {
    grid-column: 1 / -1;
    padding: 8px 2px;
    color: var(--muted);
    font-size: 11px;
  }
  .error {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 12px 0 0;
    color: var(--danger);
    font-size: 10.5px;
    line-height: 1.4;
  }
  .error :global(svg) {
    flex: none;
    margin-top: 1px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .delete {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 8px 0 4px;
    color: var(--danger);
    font-size: 11.5px;
    font-weight: 600;
  }
  .delete:hover:not(:disabled) {
    color: color-mix(in srgb, var(--danger) 80%, white);
  }
  .delete:focus-visible,
  .cancel:focus-visible,
  .save:focus-visible {
    box-shadow:
      0 0 0 2px var(--bg-raised),
      0 0 0 4px color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .spacer {
    flex: 1;
  }
  .cancel,
  .save {
    height: 32px;
    border-radius: 8px;
    font-size: 11.5px;
    font-weight: 600;
  }
  .cancel {
    padding: 0 11px;
    border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    color: var(--text);
  }
  .cancel:hover:not(:disabled) {
    background: var(--bg-hover);
  }
  .save {
    padding: 0 14px;
    background: var(--accent);
    color: var(--bg);
    font-weight: 650;
    transition:
      filter 120ms ease,
      box-shadow 120ms ease;
  }
  .save:hover:not(:disabled) {
    filter: brightness(1.06);
    box-shadow: 0 4px 14px
      color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .delete:disabled,
  .cancel:disabled,
  .save:disabled {
    cursor: default;
    opacity: 0.5;
  }
  @keyframes editor-in {
    from {
      opacity: 0;
      transform: translateY(-3px) scale(0.995);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
</style>
