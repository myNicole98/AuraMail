<script lang="ts">
  import { CalendarDays, Mail, Settings } from "lucide-svelte";
  import { app } from "../stores/app.svelte";

  let {
    onOpenSettings,
    collapsed
  }: { onOpenSettings: () => void; collapsed: boolean } = $props();
</script>

<aside class="app-rail" class:collapsed aria-label="Workspaces">
  <div class="rail-group">
    <button
      class="rail-button"
      class:active={!app.calendarOpen}
      type="button"
      aria-label="Mail"
      title="Mail"
      aria-pressed={!app.calendarOpen}
      onclick={() => app.selectMail()}
    >
      <Mail size={18} strokeWidth={1.8} />
    </button>
    <button
      class="rail-button"
      class:active={app.calendarOpen}
      type="button"
      aria-label="Calendar"
      title="Calendar"
      aria-pressed={app.calendarOpen}
      onclick={() => app.selectCalendar()}
    >
      <CalendarDays size={18} strokeWidth={1.8} />
    </button>
  </div>

  <div class="rail-spacer"></div>

  <div class="rail-settings">
    <button
      class="rail-button"
      type="button"
      aria-label="Settings"
      title="Settings"
      onclick={onOpenSettings}
    >
      <Settings size={18} strokeWidth={1.8} />
    </button>
  </div>
</aside>

<style>
  .app-rail {
    grid-column: 1;
    grid-row: 1 / -1;
    display: grid;
    grid-template-rows: auto 1fr;
    justify-items: center;
    gap: 8px;
    position: relative;
    width: 48px;
    min-width: 48px;
    height: 100%;
    padding: 10px 0;
    border-right: 0;
    box-shadow: inset -1px 0 var(--border);
    background: var(--bg-raised);
    z-index: 5;
  }

  .rail-group {
    display: grid;
    justify-items: center;
    width: 100%;
    gap: 6px;
  }

  .rail-spacer {
    width: 100%;
  }

  .rail-settings {
    position: absolute;
    right: 0;
    bottom: 10px;
    left: 0;
    display: flex;
    justify-content: center;
  }

  .app-rail.collapsed .rail-settings {
    bottom: 6px;
  }

  .rail-button {
    display: grid;
    place-items: center;
    justify-self: center;
    width: 34px;
    height: 34px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 8px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    transition:
      color 140ms ease,
      background 140ms ease,
      border-color 140ms ease;
  }

  .rail-button:hover {
    color: var(--text-strong);
    background: var(--bg-hover);
  }

  .rail-button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .rail-button.active {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 30%, var(--border));
    background: color-mix(in srgb, var(--accent) 14%, var(--bg-raised));
  }

  @media (max-width: 760px) {
    .app-rail {
      width: 42px;
      min-width: 42px;
    }

    .rail-button {
      width: 30px;
      height: 30px;
    }
  }
</style>
