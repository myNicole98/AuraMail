<script lang="ts">
  import { onMount } from "svelte";
  import { Check, ChevronDown, Settings } from "lucide-svelte";
  import SpaceIcon from "./settings/SpaceIcon.svelte";
  import { app } from "../stores/app.svelte";

  let {
    collapsed = false,
    onOpenAccountSettings
  }: {
    collapsed?: boolean;
    onOpenAccountSettings: () => void;
  } = $props();

  let open = $state(false);
  let trigger = $state<HTMLButtonElement | null>(null);
  let menuLeft = $state(0);
  let menuTop = $state(0);
  const hasAttention = $derived(
    Object.entries(app.spaceAttention).some(
      ([id, value]) => value && Number(id) !== app.activeSpaceId
    )
  );

  function choose(spaceId: number): void {
    open = false;
    void app.switchSpace(spaceId);
  }

  function toggleMenu(): void {
    if (open) {
      open = false;
      return;
    }
    const rect = trigger?.getBoundingClientRect();
    if (rect) {
      const width = Math.min(190, window.innerWidth - 16);
      const preferredLeft = collapsed ? rect.right + 6 : rect.left;
      menuLeft = Math.max(8, Math.min(preferredLeft, window.innerWidth - width - 8));
      menuTop = collapsed ? rect.top : rect.bottom + 5;
    }
    open = true;
  }

  onMount(() => {
    const close = () => {
      if (open) open = false;
    };
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
    };
  });
</script>

<div class="space-selector">
  <button
    class="space-trigger"
    class:collapsed
    class:attention={hasAttention}
    type="button"
    aria-haspopup="menu"
    aria-expanded={open}
    aria-label={`Space: ${app.activeSpace?.name ?? "Personal"}`}
    title={app.activeSpace?.name ?? "Personal"}
    bind:this={trigger}
    onclick={toggleMenu}
  >
    <SpaceIcon name={app.activeSpace?.icon ?? "layers"} size={15} />
    {#if !collapsed}
      <span class="space-name">{app.activeSpace?.name ?? "Personal"}</span>
      <ChevronDown size={13} />
    {/if}
  </button>

  {#if open}
    <div
      class="menu"
      role="menu"
      aria-label="Spaces"
      style={`left: ${menuLeft}px; top: ${menuTop}px;`}
    >
      {#each app.spaces as space (space.id)}
        <button
          class="menu-item"
          class:attention={app.spaceAttention[space.id] === true}
          type="button"
          role="menuitem"
          aria-current={space.id === app.activeSpaceId ? "true" : undefined}
          onclick={() => choose(space.id)}
        >
          <span class="space-icon"><SpaceIcon name={space.icon} size={14} /></span>
          <span class="space-name">{space.name}</span>
          {#if (app.spaceUnreadCounts[space.id] ?? 0) > 0}
            <span class="unread">{app.spaceUnreadCounts[space.id]}</span>
          {/if}
          {#if space.id === app.activeSpaceId}
            <Check size={14} />
          {/if}
        </button>
      {/each}
      <div class="menu-separator"></div>
      <button
        class="menu-item"
        type="button"
        role="menuitem"
        onclick={() => {
          open = false;
          onOpenAccountSettings();
        }}
      >
        <Settings size={14} />
        <span>Manage spaces…</span>
      </button>
    </div>
  {/if}
</div>

<svelte:window onclick={(event) => {
  const target = event.target as HTMLElement | null;
  if (!target?.closest(".space-selector")) open = false;
}} />

<style>
  .space-selector {
    position: relative;
    margin-bottom: 6px;
  }
  .space-trigger {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 34px;
    padding: 0 9px;
    border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
    border-radius: 9px;
    background: color-mix(in srgb, var(--bg-hover) 55%, transparent);
    color: var(--text-strong);
    font-size: 12.5px;
    font-weight: 600;
    text-align: left;
  }
  .space-trigger:hover {
    background: var(--bg-hover);
  }
  .space-trigger.attention {
    animation: space-blink 1.8s ease-in-out infinite;
  }
  .space-trigger.collapsed {
    justify-content: center;
    width: 36px;
    padding: 0;
  }
  .space-icon {
    display: grid;
    place-items: center;
    color: var(--accent);
  }
  .space-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .menu {
    position: fixed;
    z-index: 40;
    width: min(190px, calc(100vw - 16px));
    padding: 5px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg-raised);
    box-shadow: 0 14px 40px rgb(0 0 0 / 28%);
  }
  .menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 30px;
    padding: 0 8px;
    border-radius: 7px;
    color: var(--text);
    font-size: 12.5px;
    text-align: left;
  }
  .menu-item:hover {
    background: var(--bg-hover);
  }
  .menu-item[aria-current="true"] {
    color: var(--text-strong);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .menu-item.attention {
    animation: space-blink 1.8s ease-in-out infinite;
  }
  .unread {
    flex: none;
    min-width: 16px;
    padding: 0 4px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--bg);
    font-size: 8.5px;
    font-weight: 700;
    text-align: center;
  }
  .menu-separator {
    height: 1px;
    margin: 5px 4px;
    background: color-mix(in srgb, var(--border) 60%, transparent);
  }
  @keyframes space-blink {
    0%,
    100% {
      background: transparent;
    }
    50% {
      background: color-mix(in srgb, var(--accent) 18%, transparent);
    }
  }
</style>
