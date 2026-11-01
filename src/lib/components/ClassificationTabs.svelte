<script module lang="ts">
  // Stable across tab-strip re-renders: the clicked tab stays quiet until the
  // pointer leaves the bar.
  let suppressedTab: string | null = null;
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";
  import { flip } from "svelte/animate";
  import { ChevronLeft, ChevronRight, Mails, Plus } from "lucide-svelte";
  import { app } from "../stores/app.svelte";
  import { appearance } from "../stores/appearance.svelte";
  import type { Tab } from "../types";
  import {
    TAB_COLORS,
    tabColor,
    tabIconComponent
  } from "../tabAppearance";
  import TabEditor from "./TabEditor.svelte";
  import { insertionIndex } from "../tabOrder";

  let bar = $state<HTMLElement | null>(null);
  let scroller = $state<HTMLElement | null>(null);
  let orderedTabs = $state<Tab[]>(app.tabs);
  let overflow = $state(false);
  let canLeft = $state(false);
  let canRight = $state(false);
  let drag = $state<{
    id: number;
    startX: number;
    active: boolean;
  } | null>(null);
  let suppressClick = false;
  let committing = $state(false);
  let editorOpen = $state(false);
  let editorTab = $state<Tab | null>(null);
  let editorAnchor = $state({ x: 0, y: 0 });
  let hoverHint = $state<{ name: string; left: number; top: number } | null>(
    null
  );
  let hoverHintKey: string | null = null;
  let prefetchTimer: ReturnType<typeof setTimeout> | undefined;
  let prefetchKey: string | null = null;

  function scheduleTabPrefetch(key: string) {
    if (prefetchKey === key) return;
    prefetchKey = key;
    clearTimeout(prefetchTimer);
    const tabId = key === "all" ? null : Number(key);
    prefetchTimer = setTimeout(() => {
      void app.prefetchTab(Number.isNaN(tabId) ? null : tabId);
    }, 120);
  }

  function hoverTabs(event: PointerEvent) {
    const element = (event.target as Element | null)?.closest<HTMLElement>(
      "[data-tab-key]"
    );
    if (!element) {
      if (appearance.iconOnlyTabs) hideHint();
      return;
    }
    const key = element.dataset.tabKey ?? "";
    scheduleTabPrefetch(key);
    if (!appearance.iconOnlyTabs) return;
    if (suppressedTab === key || hoverHintKey === key) return;
    suppressedTab = null;
    const icon = element.querySelector("svg");
    const anchor = icon?.getBoundingClientRect() ?? element.getBoundingClientRect();
    hoverHint = {
      name: element.dataset.tabName ?? "",
      left: anchor.left + anchor.width / 2,
      top: anchor.top - 2
    };
    hoverHintKey = key;
  }

  function hideHint() {
    hoverHint = null;
    hoverHintKey = null;
  }

  function clearHint() {
    suppressedTab = null;
    hideHint();
    clearTimeout(prefetchTimer);
    prefetchKey = null;
  }

  $effect(() => {
    if (!appearance.iconOnlyTabs) {
      suppressedTab = null;
      hideHint();
    }
  });

  $effect(() => {
    if (drag?.active || committing) return;
    orderedTabs = app.tabs;
  });

  $effect(() => {
    orderedTabs;
    app.tabUnreadCounts;
    appearance.iconOnlyTabs;
    void tick().then(measure);
  });

  $effect(() => {
    const id = app.selectedTabId;
    if (id === null) return;
    void tick().then(() => revealTab(id));
  });

  onMount(() => {
    const observer =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(() => measure());
    if (observer) {
      if (bar) observer.observe(bar);
      if (scroller) observer.observe(scroller);
    }
    void tick().then(measure);
    return () => observer?.disconnect();
  });

  function measure() {
    if (!scroller) return;
    overflow = scroller.scrollWidth > scroller.clientWidth + 1;
    canLeft = scroller.scrollLeft > 1;
    canRight =
      scroller.scrollLeft + scroller.clientWidth < scroller.scrollWidth - 1;
  }

  function revealTab(tabId: number) {
    if (!scroller) return;
    const tab = scroller.querySelector<HTMLElement>(`[data-tab-id="${tabId}"]`);
    if (!tab) return;
    const tabRect = tab.getBoundingClientRect();
    const scrollRect = scroller.getBoundingClientRect();
    if (tabRect.left < scrollRect.left) {
      scroller.scrollBy({
        left: tabRect.left - scrollRect.left - 8,
        behavior: "smooth"
      });
    } else if (tabRect.right > scrollRect.right) {
      scroller.scrollBy({
        left: tabRect.right - scrollRect.right + 8,
        behavior: "smooth"
      });
    }
    measure();
  }

  function scrollTabs(direction: -1 | 1) {
    hideHint();
    if (!scroller) return;
    scroller.scrollBy({
      left: direction * Math.max(120, scroller.clientWidth * 0.6),
      behavior: "smooth"
    });
    measure();
  }

  function count(tabId: number): number {
    return app.unreadCountFor(tabId);
  }

  function allCount(): number {
    return app.tabUnreadCounts.all;
  }

  function badge(value: number): string {
    return value > 99 ? "99+" : String(value);
  }

  function selectTab(tabId: number | null) {
    suppressedTab = tabId === null ? "all" : String(tabId);
    hideHint();
    if (suppressClick) {
      suppressClick = false;
      return;
    }
    void app.selectTab(tabId);
  }

  function openEditor(tab: Tab, event: MouseEvent) {
    event.preventDefault();
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    editorTab = tab;
    editorAnchor = { x: rect.left, y: rect.bottom + 2 };
    editorOpen = true;
  }

  function openCreate(event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    editorTab = null;
    editorAnchor = { x: rect.left, y: rect.bottom + 2 };
    editorOpen = true;
  }

  function closeEditor() {
    editorOpen = false;
  }

  function pointerDown(tab: Tab, event: PointerEvent) {
    if (!event.isPrimary || event.button !== 0) return;
    drag = { id: tab.id, startX: event.clientX, active: false };
  }

  function pointerMove(event: PointerEvent) {
    if (!drag) return;
    const delta = event.clientX - drag.startX;
    if (!drag.active) {
      if (Math.abs(delta) < 5) return;
      drag = { ...drag, active: true };
      suppressClick = true;
      if (scroller) scroller.style.cursor = "grabbing";
    }
    autoScroll(event.clientX);
    if (!scroller) return;
    const draggingId = drag.id;
    const scrollRect = scroller.getBoundingClientRect();
    const contentX = event.clientX - scrollRect.left + scroller.scrollLeft;
    const wrappers = [
      ...scroller.querySelectorAll<HTMLElement>("[data-tab-id]")
    ].filter((element) => Number(element.dataset.tabId) !== draggingId);
    const centers = wrappers.map(
      (element) => element.offsetLeft + element.offsetWidth / 2
    );
    const target = insertionIndex(contentX, centers);
    const moving = orderedTabs.find((tab) => tab.id === draggingId);
    if (!moving) return;
    const remaining = orderedTabs.filter((tab) => tab.id !== moving.id);
    const next = [...remaining];
    next.splice(Math.min(target, next.length), 0, moving);
    if (next.every((tab, index) => tab.id === orderedTabs[index]?.id)) return;
    orderedTabs = next;
  }

  function autoScroll(clientX: number) {
    if (!scroller) return;
    const rect = scroller.getBoundingClientRect();
    if (clientX < rect.left + 24) {
      scroller.scrollLeft -= 10;
      measure();
    } else if (clientX > rect.right - 24) {
      scroller.scrollLeft += 10;
      measure();
    }
  }

  function pointerUp() {
    if (!drag) return;
    const active = drag.active;
    drag = null;
    if (scroller) scroller.style.cursor = "";
    if (!active) return;
    const ids = orderedTabs.map((tab) => tab.id);
    committing = true;
    void app.reorderTabs(ids).then((error) => {
      if (error) app.error = error;
      orderedTabs = app.tabs;
      committing = false;
    });
    setTimeout(() => {
      suppressClick = false;
    }, 0);
  }
</script>

<svelte:window onpointermove={pointerMove} onpointerup={pointerUp} onpointercancel={pointerUp} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="tabs-bar"
  class:compact={appearance.iconOnlyTabs}
  bind:this={bar}
  onpointermove={hoverTabs}
  onpointerleave={clearHint}
>
  <div
    class="tab-wrap all"
    class:active={app.selectedTabId === null}
    style:--tab-color="var(--accent)"
  >
    <button
      type="button"
      class="tab"
      role="tab"
      aria-selected={app.selectedTabId === null}
      data-tab-key="all"
      data-tab-name="All"
      onclick={() => selectTab(null)}
    >
      <Mails size={14} />
      <span class="name">All</span>
      {#if allCount() > 0}
        <span class="count">{badge(allCount())}</span>
      {/if}
    </button>
  </div>

  {#if overflow && canLeft}
    <button
      class="arrow"
      type="button"
      aria-label="Scroll tabs left"
      title="Scroll left"
      onclick={() => scrollTabs(-1)}
    >
      <ChevronLeft size={16} />
    </button>
  {/if}

  <div
    class="tab-scroll"
    bind:this={scroller}
    onscroll={() => {
      hideHint();
      measure();
    }}
  >
    {#each orderedTabs as tab, index (tab.id)}
      {@const Icon = tabIconComponent(tab)}
      {@const color = tabColor(tab, index, appearance.colorVision, appearance.themeDark)}
      <div
        class="tab-wrap"
        class:active={app.selectedTabId === tab.id}
        class:pressing={drag !== null && drag.id === tab.id && !drag.active}
        class:dragging={drag?.active === true && drag.id === tab.id}
        data-tab-id={tab.id}
        style:--tab-color={color}
        animate:flip={{ duration: appearance.reduceMotion ? 0 : 150 }}
      >
        <button
          type="button"
          class="tab"
          role="tab"
          aria-selected={app.selectedTabId === tab.id}
          title={tab.description}
          data-tab-key={tab.id}
          data-tab-name={tab.name}
          onclick={() => selectTab(tab.id)}
          oncontextmenu={(event) => openEditor(tab, event)}
          onpointerdown={(event) => pointerDown(tab, event)}
        >
          <Icon size={14} />
          <span class="name">{tab.name}</span>
          {#if count(tab.id) > 0}
            <span class="count">{badge(count(tab.id))}</span>
          {/if}
        </button>
      </div>
    {/each}
  </div>

  <button
    class="add"
    type="button"
    aria-label="Add classification tab"
    title="Add tab"
    onclick={openCreate}
  >
    <Plus size={15} />
  </button>

  {#if overflow && canRight}
    <button
      class="arrow"
      type="button"
      aria-label="Scroll tabs right"
      title="Scroll right"
      onclick={() => scrollTabs(1)}
    >
      <ChevronRight size={16} />
    </button>
  {/if}
</div>

{#if appearance.iconOnlyTabs && hoverHint}
  <div
    class="tab-hint"
    style:left={`${hoverHint.left}px`}
    style:top={`${hoverHint.top}px`}
    role="presentation"
  >
    <div class="tab-hint-inner">{hoverHint.name}</div>
  </div>
{/if}

{#if editorOpen}
  {#key editorTab?.id ?? "new"}
    <TabEditor
      tab={editorTab}
      anchor={editorAnchor}
      defaultColor={TAB_COLORS[app.tabs.length % TAB_COLORS.length]}
      onClose={closeEditor}
    />
  {/key}
{/if}

<style>
  .tabs-bar {
    display: flex;
    align-items: stretch;
    min-width: 0;
    height: 54px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .tab-scroll {
    position: relative;
    display: flex;
    align-items: stretch;
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }
  .tab-scroll::-webkit-scrollbar {
    display: none;
  }
  .tab-wrap {
    position: relative;
    display: flex;
    align-items: stretch;
    flex: none;
    margin-right: 20px;
    border-bottom: 2px solid transparent;
    color: var(--muted);
    transition: translate 130ms ease, scale 130ms ease, filter 130ms ease,
      color 130ms ease;
  }
  .tab-wrap.pressing {
    scale: 1.01;
  }
  .tab-wrap.active {
    border-bottom-color: var(--tab-color);
    color: var(--text-strong);
  }
  .tab-wrap.dragging {
    z-index: 3;
    scale: 1.03;
    filter: drop-shadow(0 5px 9px color-mix(in srgb, var(--bg) 45%, transparent));
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 100%;
    padding: 0;
    color: inherit;
    white-space: nowrap;
    cursor: pointer;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .tab:hover {
    color: var(--text-strong);
  }
  .tab:focus-visible {
    color: var(--text-strong);
  }
  .tab :global(svg) {
    flex: none;
    color: var(--tab-color);
  }
  .count {
    min-width: 13px;
    color: var(--muted);
    font-size: 9.5px;
    font-variant-numeric: tabular-nums;
    font-weight: 700;
    text-align: center;
  }
  .tab-wrap.active .count {
    color: color-mix(in srgb, var(--tab-color) 75%, var(--text));
  }
  .tab-wrap.dragging .tab {
    cursor: grabbing;
  }
  .arrow {
    display: grid;
    place-items: center;
    width: 22px;
    flex: none;
    color: var(--muted);
  }
  .arrow:hover {
    color: var(--text);
  }
  .add {
    display: grid;
    place-items: center;
    width: 28px;
    flex: none;
    margin-left: 4px;
    color: var(--muted);
  }
  .add:hover {
    color: var(--text-strong);
  }
  .all .tab :global(svg) {
    color: var(--muted);
  }
  .all.active .tab :global(svg) {
    color: var(--accent);
  }
  .tabs-bar.compact .tab {
    position: relative;
    gap: 0;
  }
  .tabs-bar.compact .tab-wrap {
    margin-right: 14px;
  }
  .tabs-bar.compact .tab-wrap:hover,
  .tabs-bar.compact .tab-wrap:focus-within {
    z-index: 4;
  }
  .tabs-bar.compact .name {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    border: 0;
  }
  .tabs-bar.compact .count {
    margin-left: 6px;
  }
  .tab-hint {
    position: fixed;
    z-index: 60;
    transform: translate(-50%, -100%);
    pointer-events: none;
  }
  .tab-hint-inner {
    padding: 2px 8px;
    border: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
    border-radius: 7px;
    background: var(--bg-raised);
    color: var(--text-strong);
    box-shadow: 0 4px 12px color-mix(in srgb, var(--bg) 55%, transparent);
    font-size: 12px;
    font-weight: 600;
    line-height: 1.25;
    white-space: nowrap;
    animation: tab-hint-in 120ms ease-out;
  }
  @keyframes tab-hint-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  :global([data-reduce-motion="true"]) .tab-hint-inner {
    animation: none;
  }
</style>
