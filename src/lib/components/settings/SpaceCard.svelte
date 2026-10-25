<script lang="ts">
  import {
    ArrowDown,
    ArrowUp,
    ChevronDown,
    ChevronRight,
    Pencil,
    Plus,
    Trash2
  } from "lucide-svelte";
  import { accountColorValue } from "../../accountColors";
  import SpaceIcon from "./SpaceIcon.svelte";
  import { appearance } from "../../stores/appearance.svelte";
  import type { Account, Space } from "../../types";

  let {
    space,
    accounts,
    active,
    selectedAccountId,
    unread = 0,
    draggedAccountId = null,
    dragTarget = false,
    collapsed = false,
    onToggle,
    onAddAccount,
    onSelectAccount,
    onRename,
    onDelete,
    onMove,
    onDragStart,
    onDragEnd,
    onDropSpace,
    onDragOverSpace,
    onDragLeaveSpace,
    onDragOverAccount,
    onDropAccount
  }: {
    space: Space;
    accounts: Account[];
    active: boolean;
    selectedAccountId: number | null;
    unread?: number;
    draggedAccountId?: number | null;
    dragTarget?: boolean;
    collapsed?: boolean;
    onToggle: () => void;
    onAddAccount: () => void;
    onSelectAccount: (account: Account) => void;
    onRename: () => void;
    onDelete: () => void;
    onMove: (account: Account, direction: -1 | 1) => void;
    onDragStart: (accountId: number) => void;
    onDragEnd: () => void;
    onDropSpace: () => void;
    onDragOverSpace: () => void;
    onDragLeaveSpace: () => void;
    onDragOverAccount: (accountId: number, before: boolean) => void;
    onDropAccount: (accountId: number) => void;
  } = $props();
</script>

<section
  class="space-card"
  class:active
  class:drop-target={dragTarget}
  role="group"
  aria-label={space.name}
  ondragover={(event) => {
    event.preventDefault();
    onDragOverSpace();
  }}
  ondragleave={(event) => {
    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
      onDragLeaveSpace();
    }
  }}
  ondrop={(event) => {
    event.preventDefault();
    onDropSpace();
  }}
>
  <header class="space-head">
    <button
      class="space-toggle"
      type="button"
      aria-expanded={!collapsed}
      onclick={onToggle}
    >
      {#if collapsed}<ChevronRight size={13} />{:else}<ChevronDown size={13} />{/if}
      <span class="space-icon"><SpaceIcon name={space.icon} size={14} /></span>
      <span class="space-name">{space.name}</span>
      {#if unread > 0}<span class="space-unread">{unread}</span>{/if}
      {#if active}<span class="active-mark">Active</span>{/if}
    </button>
    <div class="space-actions">
      <button
        class="icon-btn"
        type="button"
        aria-label={`Rename ${space.name}`}
        title="Rename space"
        onclick={onRename}
      >
        <Pencil size={13} />
      </button>
      <button
        class="icon-btn danger"
        type="button"
        aria-label={`Delete ${space.name}`}
        title="Delete space"
        onclick={onDelete}
      >
        <Trash2 size={13} />
      </button>
    </div>
  </header>

  {#if !collapsed}
    <div class="space-accounts" role="list">
      {#each accounts as account, index (account.id)}
        <div
          class="account-row"
          role="listitem"
          class:selected={selectedAccountId === account.id}
          class:dragging={draggedAccountId === account.id}
          draggable="true"
          title="Drag to reorder or move to another space"
          ondragstart={(event) => {
            event.dataTransfer?.setData("text/plain", String(account.id));
            onDragStart(account.id);
          }}
          ondragend={onDragEnd}
          ondragover={(event) => {
            event.preventDefault();
            event.stopPropagation();
            const rect = event.currentTarget.getBoundingClientRect();
            const before = event.clientY < rect.top + rect.height / 2;
            onDragOverAccount(account.id, before);
          }}
          ondrop={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onDropAccount(account.id);
          }}
        >
          <button
            class="account-select"
            type="button"
            aria-label={`Edit ${account.email}`}
            onclick={() => onSelectAccount(account)}
          >
            <span
              class="account-mark"
              style:background={accountColorValue(
                account.color,
                appearance.colorVision,
                appearance.themeDark
              )}
            >
              {account.display_name.trim().slice(0, 1).toUpperCase()}
            </span>
            <span class="account-meta">
              <strong>{account.display_name}</strong>
              <span>{account.email}</span>
            </span>
          </button>
          <div class="order-actions">
            <button
              class="icon-btn"
              type="button"
              aria-label={`Move ${account.display_name} up`}
              title="Move up"
              disabled={index === 0}
              onclick={() => onMove(account, -1)}
            >
              <ArrowUp size={12} />
            </button>
            <button
              class="icon-btn"
              type="button"
              aria-label={`Move ${account.display_name} down`}
              title="Move down"
              disabled={index === accounts.length - 1}
              onclick={() => onMove(account, 1)}
            >
              <ArrowDown size={12} />
            </button>
          </div>
        </div>
      {:else}
        <p class="empty">No accounts in this space.</p>
      {/each}
      <button class="add-account" type="button" onclick={onAddAccount}>
        <Plus size={13} />
        Add account
      </button>
    </div>
  {/if}
</section>

<style>
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
    gap: 6px;
    padding: 8px 9px;
  }
  .space-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    height: 28px;
    color: var(--text-strong);
    text-align: left;
  }
  .space-icon {
    display: grid;
    place-items: center;
    color: var(--accent);
  }
  .space-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    font-weight: 600;
  }
  .space-unread {
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
  .active-mark {
    flex: none;
    padding: 1px 5px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
    font-size: 8.5px;
    font-weight: 700;
    text-transform: uppercase;
  }
  .space-actions,
  .order-actions {
    display: flex;
    gap: 1px;
    flex: none;
  }
  .icon-btn {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    color: var(--muted);
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text);
  }
  .icon-btn.danger:hover:not(:disabled) {
    color: var(--danger);
  }
  .icon-btn:disabled {
    opacity: 0.25;
  }
  .space-accounts {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 2px 8px 9px;
  }
  .account-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    padding: 6px;
    border: 1px solid transparent;
    border-radius: 9px;
    cursor: grab;
  }
  .account-row:hover {
    background: color-mix(in srgb, var(--bg-hover) 65%, transparent);
  }
  .account-row.selected {
    background: color-mix(in srgb, var(--accent) 10%, var(--bg-raised));
  }
  .account-row.dragging {
    cursor: grabbing;
    opacity: 0.45;
  }
  .account-select {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    text-align: left;
  }
  .account-mark {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    flex: none;
    border-radius: 7px;
    color: #1f2430;
    font-size: 10px;
    font-weight: 650;
  }
  .account-meta {
    display: flex;
    flex-direction: column;
    gap: 0;
    min-width: 0;
  }
  .account-meta strong,
  .account-meta span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .account-meta strong {
    color: var(--text-strong);
    font-size: 11px;
    font-weight: 600;
  }
  .account-meta span {
    color: var(--muted);
    font-size: 9.5px;
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
    color: var(--muted);
    font-size: 11px;
  }
  .add-account:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .empty {
    margin: 2px 4px 4px;
    color: var(--muted);
    font-size: 10px;
  }
</style>
