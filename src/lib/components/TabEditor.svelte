<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { Search, Trash2 } from "lucide-svelte";
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

  let icons = $derived(tabIconSearch(iconQuery));
  let left = $derived(
    Math.max(12, Math.min(anchor.x, (window?.innerWidth ?? 1280) - 356))
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
    if (!trimmedName || !trimmedDescription) {
      error = "Name and description are required.";
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
  class:create={!tab}
  bind:this={editor}
  role="dialog"
  aria-label={tab ? "Edit tab" : "New tab"}
  style:left={`${left}px`}
  style:top={`${top}px`}
  style:--tab-color={previewColor(color)}
>
  <form onsubmit={save}>
    <div class="editor-head">
      <strong>{tab ? "Edit tab" : "New tab"}</strong>
      {#if tab}<span>{tab.name}</span>{/if}
    </div>

    <div class="field">
      <label for="tab-name">Name</label>
      <input
        id="tab-name"
        class="name-input"
        bind:this={nameInput}
        bind:value={name}
        maxlength="60"
        autocomplete="off"
        spellcheck="false"
      />
    </div>

    <div class="field">
      <label for="tab-description">Description</label>
      <textarea
        id="tab-description"
        class="description"
        rows="2"
        bind:value={description}
        maxlength="500"
        placeholder="Describe which mail belongs here"
      ></textarea>
    </div>

    <div class="field">
      <span class="field-label">Color</span>
      <div class="swatches">
        {#each TAB_COLORS as swatch, index (swatch)}
          <button
            type="button"
            class="swatch"
            class:active={color.toLowerCase() === swatch}
            style:--c={previewColor(swatch, index)}
            aria-label={`Use ${swatch}`}
            title={swatch}
            onclick={() => (color = swatch)}
          ></button>
        {/each}
        <label class="swatch custom" title="Custom color">
          <input
            type="color"
            bind:value={color}
            aria-label="Custom tab color"
          />
          <span
            class="custom-preview"
            style:--c={previewColor(color)}
            aria-hidden="true"
          ></span>
        </label>
      </div>
    </div>

    <div class="field">
      <span class="field-label">Icon</span>
      <label class="icon-search">
        <Search size={13} />
        <input
          bind:value={iconQuery}
          aria-label="Search tab icons"
          placeholder="Search icons"
          spellcheck="false"
          onkeydown={(event) => {
            if (event.key === "Enter") event.preventDefault();
          }}
        />
      </label>
      <div class="icon-grid">
        {#each icons as option (option.id)}
          {@const Icon = option.component}
          <button
            type="button"
            class="icon-button"
            class:active={icon === option.id}
            aria-label={option.label}
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

    {#if error}<p class="error" role="alert">{error}</p>{/if}

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
      <button type="submit" class="save" disabled={saving}>
        {saving ? "Saving…" : tab ? "Save" : "Create tab"}
      </button>
    </div>
  </form>
</div>

<style>
  .editor {
    position: fixed;
    z-index: 120;
    width: min(344px, calc(100vw - 24px));
    padding: 16px;
    border: 1px solid color-mix(in srgb, var(--border) 85%, transparent);
    border-radius: 13px;
    background: var(--bg-raised);
    box-shadow: 0 18px 44px color-mix(in srgb, var(--bg) 55%, black);
    color: var(--text);
  }
  .editor-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 14px;
  }
  .editor-head strong {
    color: var(--text-strong);
    font-size: 13px;
    font-weight: 650;
  }
  .editor-head span {
    overflow: hidden;
    color: var(--muted);
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .field {
    margin-bottom: 13px;
  }
  .field label,
  .field-label {
    display: block;
    margin-bottom: 5px;
    color: var(--muted);
    font-size: 9.5px;
    font-weight: 650;
    letter-spacing: 0.045em;
    text-transform: uppercase;
  }
  .name-input {
    width: 100%;
    height: 30px;
    padding: 0;
    border: 0;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    background: transparent;
    color: var(--text-strong);
    font: inherit;
    font-size: 15px;
    font-weight: 600;
    outline: 0;
  }
  .name-input:focus {
    border-bottom-color: var(--tab-color);
  }
  .description {
    width: 100%;
    min-height: 54px;
    padding: 8px 9px;
    border: 0;
    border-radius: 8px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    color: var(--text);
    font: inherit;
    font-size: 11.5px;
    line-height: 1.45;
    resize: vertical;
    outline: 0;
  }
  .description:focus {
    box-shadow: inset 0 0 0 1px var(--tab-color);
  }
  .swatches {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .swatch {
    position: relative;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 2px solid transparent;
    border-radius: 50%;
    background: var(--c);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--c) 50%, transparent);
  }
  .swatch.active {
    border-color: var(--bg-raised);
    box-shadow: 0 0 0 2px var(--c);
  }
  .swatch.custom {
    display: grid;
    place-items: center;
    overflow: hidden;
    background: conic-gradient(
      #e6797e,
      #f2b85b,
      #58c98b,
      #6da4ff,
      #b489ff,
      #e6797e
    );
    box-shadow: none;
    color: #10141b;
    cursor: pointer;
    font-size: 9px;
    font-weight: 800;
  }
  .swatch.custom input {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
    pointer-events: none;
  }
  .custom-preview {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--c);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--bg-raised) 70%, transparent);
  }
  .icon-search {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 6px;
    padding: 0 7px;
    border-radius: 7px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    color: var(--muted);
  }
  .icon-search input {
    width: 100%;
    height: 27px;
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
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    max-height: 106px;
    overflow-y: auto;
  }
  .icon-button {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 7px;
    color: var(--muted);
  }
  .icon-button:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .icon-button.active {
    background: color-mix(in srgb, var(--tab-color) 16%, transparent);
    color: var(--tab-color);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--tab-color) 45%, transparent);
  }
  .no-icons {
    padding: 7px 2px;
    color: var(--muted);
    font-size: 11px;
  }
  .error {
    margin: 0 0 8px;
    color: var(--danger);
    font-size: 11px;
    line-height: 1.4;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 16px;
  }
  .delete {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 29px;
    padding: 0 8px 0 4px;
    color: var(--danger);
    font-size: 11.5px;
    font-weight: 600;
  }
  .delete:hover:not(:disabled) {
    color: color-mix(in srgb, var(--danger) 80%, white);
  }
  .spacer {
    flex: 1;
  }
  .cancel {
    height: 29px;
    padding: 0 10px;
    border-radius: 7px;
    color: var(--muted);
    font-size: 11.5px;
    font-weight: 600;
  }
  .cancel:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text);
  }
  .save {
    height: 29px;
    padding: 0 12px;
    border-radius: 7px;
    background: var(--tab-color);
    color: var(--bg);
    font-size: 11.5px;
    font-weight: 650;
  }
  .save:hover:not(:disabled) {
    filter: brightness(1.08);
  }
  .delete:disabled,
  .cancel:disabled,
  .save:disabled {
    cursor: default;
    opacity: 0.5;
  }
</style>
