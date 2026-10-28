<script lang="ts">
  import { CalendarDays, CircleAlert, Plus, Trash2 } from "lucide-svelte";
  import { api } from "../../api";
  import { app } from "../../stores/app.svelte";
  import type { CalendarAccount } from "../../types";
  import CalendarAccountDialog from "../calendar/CalendarAccountDialog.svelte";
  import SpaceIcon from "./SpaceIcon.svelte";

  let accountsBySpace = $state<Record<number, CalendarAccount[]>>({});
  let loading = $state(true);
  let error = $state<string | null>(null);
  let showAddFor = $state<number | null>(null);
  let removingId = $state<number | null>(null);
  let draggedAccountId = $state<number | null>(null);
  let dragTargetSpaceId = $state<number | null>(null);
  let loadedKey = "";

  const spaces = $derived(
    [...app.spaces].sort((left, right) => left.position - right.position)
  );

  async function load(): Promise<void> {
    const current = spaces;
    if (current.length === 0) {
      accountsBySpace = {};
      loading = false;
      return;
    }
    loading = true;
    error = null;
    try {
      const entries = await Promise.all(
        current.map(async (space) => {
          const accounts = await api.listCalendarAccounts(space.id);
          return [space.id, accounts] as const;
        })
      );
      accountsBySpace = Object.fromEntries(entries);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading = false;
    }
  }

  async function remove(account: CalendarAccount): Promise<void> {
    if (removingId !== null) return;
    if (
      !window.confirm(
        `Remove calendar account “${account.displayName}” and its cached calendars?`
      )
    ) {
      return;
    }
    removingId = account.id;
    error = null;
    try {
      await api.removeCalendarAccount(account.id);
      window.dispatchEvent(new CustomEvent("auramail:calendar-accounts-changed"));
      await load();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      removingId = null;
    }
  }

  async function added(): Promise<void> {
    showAddFor = null;
    window.dispatchEvent(new CustomEvent("auramail:calendar-accounts-changed"));
    await load();
  }

  function orderedAccounts(spaceId: number): CalendarAccount[] {
    return [...(accountsBySpace[spaceId] ?? [])].sort(
      (left, right) => left.position - right.position || left.id - right.id
    );
  }

  function setOrderedAccounts(spaceId: number, accounts: CalendarAccount[]): void {
    accountsBySpace = {
      ...accountsBySpace,
      [spaceId]: accounts.map((account, position) => ({ ...account, position }))
    };
  }

  function findAccount(accountId: number): CalendarAccount | null {
    for (const accounts of Object.values(accountsBySpace)) {
      const account = accounts.find((candidate) => candidate.id === accountId);
      if (account) return account;
    }
    return null;
  }

  function startDragging(accountId: number): void {
    draggedAccountId = accountId;
    dragTargetSpaceId = null;
  }

  function dragOverSpace(spaceId: number): void {
    if (draggedAccountId === null) return;
    const dragged = findAccount(draggedAccountId);
    dragTargetSpaceId =
      dragged && dragged.spaceId !== spaceId ? spaceId : null;
  }

  function dragOverAccount(
    spaceId: number,
    targetId: number,
    before: boolean
  ): void {
    const draggedId = draggedAccountId;
    if (draggedId === null || draggedId === targetId) return;
    const dragged = findAccount(draggedId);
    if (!dragged) return;
    if (dragged.spaceId !== spaceId) {
      dragTargetSpaceId = spaceId;
      return;
    }
    const accounts = orderedAccounts(spaceId);
    const remaining = accounts.filter((account) => account.id !== draggedId);
    const targetIndex = remaining.findIndex((account) => account.id === targetId);
    if (targetIndex < 0) return;
    remaining.splice(before ? targetIndex : targetIndex + 1, 0, dragged);
    if (remaining.every((account, index) => accounts[index]?.id === account.id)) {
      return;
    }
    setOrderedAccounts(spaceId, remaining);
  }

  async function persistOrder(spaceId: number): Promise<void> {
    try {
      const reordered = await api.reorderCalendarAccounts(
        spaceId,
        orderedAccounts(spaceId).map((account) => account.id)
      );
      setOrderedAccounts(spaceId, reordered);
      window.dispatchEvent(new CustomEvent("auramail:calendar-accounts-changed"));
      error = null;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
      await load();
    }
  }

  async function dropOnAccount(
    spaceId: number,
    targetId: number
  ): Promise<void> {
    const draggedId = draggedAccountId;
    draggedAccountId = null;
    dragTargetSpaceId = null;
    if (draggedId === null) return;
    const dragged = findAccount(draggedId);
    if (!dragged) return;
    if (dragged.spaceId === spaceId) {
      await persistOrder(spaceId);
      return;
    }
    try {
      const moved = await api.moveCalendarAccount(draggedId, spaceId);
      await load();
      const targetAccounts = orderedAccounts(spaceId).filter(
        (account) => account.id !== moved.id
      );
      const targetIndex = targetAccounts.findIndex(
        (account) => account.id === targetId
      );
      targetAccounts.splice(targetIndex < 0 ? targetAccounts.length : targetIndex, 0, moved);
      const reordered = await api.reorderCalendarAccounts(
        spaceId,
        targetAccounts.map((account) => account.id)
      );
      setOrderedAccounts(spaceId, reordered);
      window.dispatchEvent(new CustomEvent("auramail:calendar-accounts-changed"));
      error = null;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
      await load();
    }
  }

  async function dropOnSpace(spaceId: number): Promise<void> {
    const draggedId = draggedAccountId;
    draggedAccountId = null;
    dragTargetSpaceId = null;
    if (draggedId === null) return;
    const dragged = findAccount(draggedId);
    if (!dragged) return;
    if (dragged.spaceId === spaceId) {
      await persistOrder(spaceId);
      return;
    }
    try {
      await api.moveCalendarAccount(draggedId, spaceId);
      await load();
      window.dispatchEvent(new CustomEvent("auramail:calendar-accounts-changed"));
      error = null;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  $effect(() => {
    const key = spaces.map((space) => space.id).join(",");
    if (key === loadedKey) return;
    loadedKey = key;
    void load();
  });
</script>

<div class="settings-columns">
  <section class="settings-column" aria-label="CalDAV calendars">
    <header class="column-head">
      <span>CalDAV - Calendar</span>
    </header>

    {#if error}
      <p class="alert" role="alert"><CircleAlert size={14} />{error}</p>
    {/if}

    <div class="space-stack">
      {#each spaces as space (space.id)}
        {@const accounts = orderedAccounts(space.id)}
        <section
          class="space-card"
          class:active={space.id === app.activeSpaceId}
          class:drop-target={dragTargetSpaceId === space.id}
          aria-label={space.name}
          ondragover={(event) => {
            event.preventDefault();
            dragOverSpace(space.id);
          }}
          ondragleave={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
              if (dragTargetSpaceId === space.id) dragTargetSpaceId = null;
            }
          }}
          ondrop={(event) => {
            event.preventDefault();
            void dropOnSpace(space.id);
          }}
        >
          <header class="space-head">
            <span class="space-icon"><SpaceIcon name={space.icon} size={14} /></span>
            <span class="space-name">{space.name}</span>
            {#if space.id === app.activeSpaceId}
              <span class="active-mark">Active</span>
            {/if}
          </header>

          <div class="space-accounts" role="list">
            {#if loading && accounts.length === 0}
              <p class="empty">Loading calendars…</p>
            {:else if accounts.length === 0}
              <p class="empty">No calendars in this space.</p>
            {:else}
              {#each accounts as account (account.id)}
                <div
                  class="account-row"
                  class:dragging={draggedAccountId === account.id}
                  role="listitem"
                  draggable="true"
                  title="Drag to reorder or move to another space"
                  ondragstart={(event) => {
                    event.dataTransfer?.setData("text/plain", String(account.id));
                    startDragging(account.id);
                  }}
                  ondragend={() => {
                    draggedAccountId = null;
                    dragTargetSpaceId = null;
                  }}
                  ondragover={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    const rect = event.currentTarget.getBoundingClientRect();
                    dragOverAccount(
                      space.id,
                      account.id,
                      event.clientY < rect.top + rect.height / 2
                    );
                  }}
                  ondrop={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    void dropOnAccount(space.id, account.id);
                  }}
                >
                  <span class="account-symbol"><CalendarDays size={15} /></span>
                  <span class="account-meta">
                    <strong>{account.displayName}</strong>
                    <span>{account.username} · {account.baseUrl}</span>
                  </span>
                  <button
                    class="icon-btn danger"
                    type="button"
                    aria-label={`Remove ${account.displayName}`}
                    title="Remove calendar account"
                    disabled={removingId !== null}
                    onclick={() => void remove(account)}
                  >
                    <Trash2 size={13} />
                  </button>
                </div>
              {/each}
            {/if}
            <button
              class="add-account"
              type="button"
              onclick={() => (showAddFor = space.id)}
            >
              <Plus size={13} />
              Add calendar account
            </button>
          </div>
        </section>
      {/each}
    </div>
  </section>

  <section class="settings-column contacts-column" aria-label="Contacts">
    <header class="column-head">
      <span>Contacts</span>
    </header>
    <div class="contacts-placeholder">placeholder</div>
  </section>
</div>

{#if showAddFor !== null}
  <CalendarAccountDialog
    spaceId={showAddFor}
    onClose={() => (showAddFor = null)}
    onAdded={added}
  />
{/if}

<style>
  .settings-columns {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 22px;
    align-items: start;
    margin-top: 22px;
  }

  .settings-column {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 10px;
  }

  .column-head {
    display: flex;
    align-items: center;
    min-height: 22px;
    color: var(--text-strong);
    font-size: 12px;
    font-weight: 650;
    letter-spacing: 0.01em;
  }

  .alert {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--danger);
    font-size: 11px;
  }

  .space-stack {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .space-card {
    position: relative;
    border: 1px solid color-mix(in srgb, var(--border) 72%, transparent);
    border-radius: 12px;
    background: color-mix(in srgb, var(--bg-raised) 70%, var(--bg));
  }

  .space-card.active {
    border-color: color-mix(in srgb, var(--accent) 35%, var(--border));
  }

  .space-card.drop-target {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 8%, var(--bg-raised));
  }

  .space-head {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 8px 9px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 58%, transparent);
  }

  .space-icon {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    flex: none;
    border-radius: 6px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    color: var(--accent);
  }

  .space-name {
    min-width: 0;
    overflow: hidden;
    color: var(--text-strong);
    font-size: 12px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .active-mark {
    flex: none;
    padding: 2px 5px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 13%, transparent);
    color: var(--accent);
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .space-accounts {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px 8px 9px;
  }

  .account-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px;
    border: 1px solid transparent;
    border-radius: 8px;
    cursor: grab;
  }

  .account-row:hover {
    background: var(--bg-hover);
  }

  .account-row.dragging {
    opacity: 0.45;
  }

  .account-symbol {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex: none;
    border-radius: 7px;
    background: color-mix(in srgb, var(--bg-raised) 80%, var(--bg));
    color: var(--muted);
  }

  .account-meta {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .account-meta strong {
    overflow: hidden;
    color: var(--text);
    font-size: 11px;
    font-weight: 550;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .account-meta span {
    overflow: hidden;
    color: var(--muted);
    font-size: 9px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .icon-btn {
    display: grid;
    place-items: center;
    width: 25px;
    height: 25px;
    flex: none;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }

  .icon-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text);
  }

  .icon-btn.danger:hover:not(:disabled) {
    color: var(--danger);
  }

  .icon-btn:disabled {
    cursor: default;
    opacity: 0.45;
  }

  .empty {
    margin: 0;
    padding: 7px 6px;
    color: var(--muted);
    font-size: 10px;
  }

  .add-account {
    display: flex;
    align-items: center;
    gap: 7px;
    min-height: 30px;
    margin-top: 2px;
    padding: 0 8px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--muted);
    font-size: 11px;
    cursor: pointer;
  }

  .add-account:hover {
    color: var(--text);
    background: var(--bg-hover);
  }

  .contacts-placeholder {
    display: grid;
    place-items: center;
    min-height: 250px;
    border: 1px dashed color-mix(in srgb, var(--border) 70%, transparent);
    border-radius: 12px;
    color: var(--muted);
    font-size: 12px;
  }

  @media (max-width: 820px) {
    .settings-columns {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
