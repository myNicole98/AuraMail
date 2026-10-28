<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { CalendarDays, X } from "lucide-svelte";
  import { api } from "../../api";
  import type { CalendarAccount } from "../../types";

  let {
    spaceId,
    onClose,
    onAdded
  }: {
    spaceId: number;
    onClose: () => void;
    onAdded: (account: CalendarAccount) => void;
  } = $props();

  let displayName = $state("");
  let baseUrl = $state("");
  let username = $state("");
  let password = $state("");
  let error = $state<string | null>(null);
  let saving = $state(false);
  let panel = $state<HTMLElement | null>(null);
  let previousFocus: HTMLElement | null = null;

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
      return;
    }
    if (event.key !== "Tab" || !panel) return;
    const focusables = Array.from(
      panel.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])'
      )
    );
    if (focusables.length === 0) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    const active = document.activeElement;
    if (event.shiftKey && (active === first || !panel.contains(active))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (active === last || !panel.contains(active))) {
      event.preventDefault();
      first.focus();
    }
  }

  async function submit(): Promise<void> {
    if (saving) return;
    error = null;
    saving = true;
    try {
      const account = await api.addCalendarAccount({
        displayName: displayName.trim(),
        baseUrl: baseUrl.trim(),
        username: username.trim(),
        password,
        spaceId
      });
      onAdded(account);
    } catch (caught) {
      error = caught instanceof Error ? caught.message : String(caught);
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    previousFocus =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    void tick().then(() => panel?.querySelector<HTMLElement>("input")?.focus());
  });

  onDestroy(() => previousFocus?.focus());
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(event) => {
    if (event.target === event.currentTarget) onClose();
  }}
>
  <div
    class="dialog-panel"
    role="dialog"
    aria-modal="true"
    aria-labelledby="calendar-account-title"
    tabindex="-1"
    bind:this={panel}
    onkeydown={handleKeydown}
  >
    <div class="dialog-title-row">
      <div class="dialog-title">
        <CalendarDays size={17} />
        <h2 id="calendar-account-title">Add calendar account</h2>
      </div>
      <button class="icon-button" type="button" aria-label="Close" onclick={onClose}>
        <X size={16} />
      </button>
    </div>

    <form
      class="account-form"
      onsubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <label class="field">
        <span>Display name</span>
        <input bind:value={displayName} autocomplete="name" placeholder="Optional" />
      </label>
      <label class="field">
        <span>CalDAV URL</span>
        <input
          bind:value={baseUrl}
          autocomplete="url"
          spellcheck="false"
          placeholder="https://caldav.example.com/"
          required
        />
      </label>
      <label class="field">
        <span>Username</span>
        <input bind:value={username} autocomplete="username" required />
      </label>
      <label class="field">
        <span>Password / app token</span>
        <input
          type="password"
          bind:value={password}
          autocomplete="current-password"
          required
        />
        <small class="field-hint">Use an app-specific password if your provider requires one.</small>
      </label>

      {#if error}<p class="error" role="alert">{error}</p>{/if}

      <div class="form-actions">
        <button type="button" class="ghost" onclick={onClose}>Cancel</button>
        <button type="submit" class="primary" disabled={saving}>
          {saving ? "Adding…" : "Add calendar account"}
        </button>
      </div>
    </form>
  </div>
</div>

<style>
  .dialog-backdrop {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: grid;
    place-items: center;
    padding: 20px;
    background: color-mix(in srgb, #000 46%, transparent);
  }
  .dialog-panel {
    width: min(400px, 100%);
    padding: 20px;
    border: 1px solid var(--pane-border);
    border-radius: 14px;
    background: var(--bg-raised);
    box-shadow: var(--pane-highlight), var(--pane-shadow);
    color: var(--text);
  }
  .dialog-title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 16px;
  }
  .dialog-title {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-strong);
  }
  .dialog-title h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
  }
  .icon-button {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 7px;
    background: transparent;
    color: var(--muted);
  }
  .icon-button:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .account-form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    color: var(--muted);
    font-size: 11px;
  }
  .field input {
    height: 34px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    color: var(--text);
    font-size: 12px;
  }
  .field input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .field-hint {
    color: var(--muted);
    font-size: 10px;
    line-height: 1.4;
  }

  .error {
    margin: 0;
    color: var(--danger);
    font-size: 11px;
  }
  .form-actions {
    display: flex;
    justify-content: flex-end;
    gap: 7px;
    margin-top: 3px;
  }
  .form-actions button {
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    font-size: 12px;
  }
  .ghost {
    background: transparent;
    color: var(--text);
  }
  .ghost:hover {
    background: var(--bg-hover);
  }
  .primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--bg);
    font-weight: 600;
  }
  .primary:disabled {
    opacity: 0.55;
  }
</style>
