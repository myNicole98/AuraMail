<script lang="ts">
  import { onMount } from "svelte";
  import {
    ArrowDown,
    ArrowUp,
    Pencil,
    Plus,
    Tag,
    Trash2
  } from "lucide-svelte";
  import { api } from "../../api";
  import { app } from "../../stores/app.svelte";
  import { appearance } from "../../stores/appearance.svelte";
  import { tabColor, tabIconComponent } from "../../tabAppearance";
  import TabEditor from "../TabEditor.svelte";
  import ThemedSelect from "../ThemedSelect.svelte";
  import SettingsRow from "./SettingsRow.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import type { Label, Tab } from "../../types";
  import { errorMessage } from "./errors";

  let spaceId = $state<number>(app.activeSpaceId ?? app.spaces[0]?.id ?? 1);
  let tabs = $state<Tab[]>([]);
  let labels = $state<Label[]>([]);
  let editor = $state<{ tab: Tab | null } | null>(null);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let newLabelName = $state("");
  let newLabelColor = $state("#5b9bd5");

  const spaceOptions = $derived(
    app.spaces.map((space) => ({ value: String(space.id), label: space.name }))
  );

  async function loadTabs() {
    try {
      tabs = await api.listTabs(spaceId);
    } catch (caught) {
      error = errorMessage(caught);
    }
  }

  async function loadLabels() {
    try {
      labels = await api.listLabels();
      app.labels = labels;
    } catch (caught) {
      error = errorMessage(caught);
    }
  }

  onMount(() => {
    void loadLabels();
  });

  $effect(() => {
    spaceId;
    void loadTabs();
  });

  function noticeOnce(message: string) {
    notice = message;
    setTimeout(() => {
      if (notice === message) notice = null;
    }, 2400);
  }

  async function afterTabChange() {
    await loadTabs();
    if (spaceId === app.activeSpaceId) await app.refreshTabs();
  }

  async function removeTab(tab: Tab) {
    if (!window.confirm(`Delete the "${tab.name}" category?`)) return;
    try {
      await api.deleteTab(tab.id, spaceId);
      await afterTabChange();
      await app.reloadMessages();
      noticeOnce("Category deleted");
    } catch (caught) {
      error = errorMessage(caught);
    }
  }

  async function moveTab(tab: Tab, delta: number) {
    const ids = tabs.map((item) => item.id);
    const index = ids.indexOf(tab.id);
    const next = index + delta;
    if (index < 0 || next < 0 || next >= ids.length) return;
    [ids[index], ids[next]] = [ids[next], ids[index]];
    try {
      await api.reorderTabs(ids, spaceId);
      await afterTabChange();
    } catch (caught) {
      error = errorMessage(caught);
    }
  }

  async function createLabel() {
    if (!newLabelName.trim()) return;
    try {
      await api.saveLabel(null, newLabelName.trim(), newLabelColor);
      newLabelName = "";
      await loadLabels();
      noticeOnce("Label created");
    } catch (caught) {
      error = errorMessage(caught);
    }
  }

  async function renameLabel(label: Label, name: string) {
    if (!name.trim() || name === label.name) return;
    try {
      await api.saveLabel(label.id, name.trim(), label.color);
      await loadLabels();
    } catch (caught) {
      error = errorMessage(caught);
    }
  }

  async function recolorLabel(label: Label, color: string) {
    try {
      await api.saveLabel(label.id, label.name, color);
      await loadLabels();
    } catch (caught) {
      error = errorMessage(caught);
    }
  }

  async function removeLabel(label: Label) {
    if (!window.confirm(`Delete the "${label.name}" label?`)) return;
    try {
      await api.deleteLabel(label.id);
      await loadLabels();
      noticeOnce("Label deleted");
    } catch (caught) {
      error = errorMessage(caught);
    }
  }
</script>

<div class="settings-inner">
  <SettingsSection title="Category tabs">
    {#if app.spaces.length > 1}
      <SettingsRow label="Space">
        <div class="select">
          <ThemedSelect
            value={String(spaceId)}
            options={spaceOptions}
            onchange={(value) => (spaceId = Number(value))}
            ariaLabel="Space"
          />
        </div>
      </SettingsRow>
    {/if}

    {#if editor}
      <TabEditor
        embedded
        tab={editor.tab}
        spaceId={spaceId}
        onClose={() => {
          editor = null;
          void afterTabChange();
        }}
      />
    {:else if tabs.length === 0}
      <p class="empty">No categories yet.</p>
    {:else}
      {#each tabs as tab, index (tab.id)}
        {@const TabIcon = tabIconComponent(tab)}
        {@const color = tabColor(tab, index, appearance.colorVision, appearance.themeDark)}
        <div class="entry">
          <span class="badge" style:color data-tab-color={color}>
            <TabIcon size={15} />
          </span>
          <span class="entry-name">{tab.name}</span>
          <span class="entry-actions">
            <button
              type="button"
              class="entry-icon"
              disabled={index === 0}
              onclick={() => void moveTab(tab, -1)}
              aria-label={`Move ${tab.name} up`}
            >
              <ArrowUp size={15} />
            </button>
            <button
              type="button"
              class="entry-icon"
              disabled={index === tabs.length - 1}
              onclick={() => void moveTab(tab, 1)}
              aria-label={`Move ${tab.name} down`}
            >
              <ArrowDown size={15} />
            </button>
            <button
              type="button"
              class="entry-icon"
              onclick={() => (editor = { tab })}
              aria-label={`Edit ${tab.name}`}
            >
              <Pencil size={15} />
            </button>
            <button
              type="button"
              class="entry-icon danger"
              onclick={() => void removeTab(tab)}
              aria-label={`Delete ${tab.name}`}
            >
              <Trash2 size={15} />
            </button>
          </span>
        </div>
      {/each}
    {/if}

    {#if !editor}
      <div class="section-actions">
        <button type="button" class="ghost" onclick={() => (editor = { tab: null })}>
          <Plus size={14} /> Add category
        </button>
      </div>
    {/if}
  </SettingsSection>

  <SettingsSection title="Labels">
    {#each labels as label (label.id)}
      <div class="entry">
        <span class="label-tag">
          <label class="tag-icon" style:color={label.color} title={`${label.name} colour`}>
            <Tag size={13} />
            <input
              class="tag-color"
              type="color"
              value={label.color}
              aria-label={`${label.name} colour`}
              onchange={(event) => void recolorLabel(label, event.currentTarget.value)}
            />
          </label>
          <input
            class="tag-name"
            value={label.name}
            aria-label={`${label.name} name`}
            onchange={(event) => void renameLabel(label, event.currentTarget.value)}
          />
        </span>
        <span class="entry-actions">
          <button
            type="button"
            class="entry-icon danger"
            onclick={() => void removeLabel(label)}
            aria-label={`Delete ${label.name}`}
          >
            <Trash2 size={15} />
          </button>
        </span>
      </div>
    {/each}

    <div class="entry">
      <span class="label-tag">
        <label class="tag-icon" style:color={newLabelColor} title="New label colour">
          <Tag size={13} />
          <input
            class="tag-color"
            type="color"
            bind:value={newLabelColor}
            aria-label="New label colour"
          />
        </label>
        <input
          class="tag-name"
          bind:value={newLabelName}
          placeholder="New label name"
          aria-label="New label name"
        />
      </span>
      <span class="entry-actions">
        <button type="button" class="ghost" onclick={() => void createLabel()}>
          <Plus size={14} /> Add label
        </button>
      </span>
    </div>
  </SettingsSection>

  {#if error}
    <p class="banner error">{error}</p>
  {/if}
  {#if notice}
    <p class="banner ok">{notice}</p>
  {/if}
</div>

<style>
  .select {
    width: 200px;
  }
  .entry {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding: 6px 0;
  }
  .entry + .entry {
    border-top: 1px solid color-mix(in srgb, var(--border) 45%, transparent);
  }
  .badge {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex: none;
    border-radius: 8px;
    background: color-mix(in srgb, currentColor 16%, transparent);
  }
  .entry-name {
    flex: 1;
    min-width: 0;
    color: var(--text);
    font-size: 12.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .entry-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: none;
  }
  .entry-icon {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--muted);
  }
  .entry-icon:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text);
  }
  .entry-icon:disabled {
    opacity: 0.35;
  }
  .danger {
    color: var(--danger);
  }
  .empty {
    margin: 0;
    padding: 10px 0;
    color: var(--muted);
    font-size: 11.5px;
  }
  .section-actions {
    display: flex;
    padding-top: 12px;
  }
  .label-tag {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: 0 1 300px;
    min-width: 0;
    height: 28px;
    padding: 0 8px 0 6px;
    border-radius: 6px;
    border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    background: var(--bg);
  }
  .tag-icon {
    position: relative;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    flex: none;
    cursor: pointer;
  }
  .tag-icon .tag-color {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }
  .tag-name {
    flex: 1;
    min-width: 60px;
    height: 22px !important;
    padding: 0 !important;
    border: 0 !important;
    background: transparent !important;
    color: var(--text);
    font-size: 12px;
  }
  .tag-name:focus {
    outline: 0;
  }
  .tag-name::placeholder {
    color: var(--muted);
  }
  .banner {
    margin: 0;
    padding: 8px 12px;
    border-radius: 8px;
    font-size: 0.82rem;
  }
  .banner.error {
    background: color-mix(in srgb, var(--danger) 14%, transparent);
    color: var(--danger);
  }
  .banner.ok {
    background: color-mix(in srgb, var(--success) 14%, transparent);
    color: var(--success);
  }
</style>
