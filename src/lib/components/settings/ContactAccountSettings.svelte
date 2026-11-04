<script lang="ts">
  import { CircleAlert, Plus, Trash2, Users } from "lucide-svelte";
  import { api } from "../../api";
  import { app } from "../../stores/app.svelte";
  import type { ContactAccount } from "../../types";
  import ContactAccountDialog from "../contacts/ContactAccountDialog.svelte";
  import SpaceIcon from "./SpaceIcon.svelte";

  let accountsBySpace = $state<Record<number, ContactAccount[]>>({});
  let loading = $state(true);
  let error = $state<string | null>(null);
  let showAddFor = $state<number | null>(null);
  let removingId = $state<number | null>(null);
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
          const accounts = await api.listContactAccounts(space.id);
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

  function orderedAccounts(spaceId: number): ContactAccount[] {
    return [...(accountsBySpace[spaceId] ?? [])].sort(
      (left, right) => left.position - right.position || left.id - right.id
    );
  }

  async function remove(account: ContactAccount): Promise<void> {
    if (removingId !== null) return;
    if (
      !window.confirm(
        `Remove contact account “${account.displayName}” and its cached contacts?`
      )
    ) {
      return;
    }
    removingId = account.id;
    error = null;
    try {
      await api.removeContactAccount(account.id);
      window.dispatchEvent(new CustomEvent("auramail:contacts-changed"));
      await load();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      removingId = null;
    }
  }

  async function added(): Promise<void> {
    showAddFor = null;
    window.dispatchEvent(new CustomEvent("auramail:contacts-changed"));
    await load();
  }

  $effect(() => {
    const key = spaces.map((space) => space.id).join(",");
    if (key === loadedKey) return;
    loadedKey = key;
    void load();
  });
</script>

<section class="settings-column" aria-label="CardDAV contacts">
  <header class="column-head">
    <span>CardDAV - Contacts</span>
  </header>

  {#if error}
    <p class="alert" role="alert"><CircleAlert size={13} />{error}</p>
  {/if}

  <div class="space-stack">
    {#each spaces as space (space.id)}
      {@const accounts = orderedAccounts(space.id)}
      <section
        class="space-card"
        class:active={space.id === app.activeSpaceId}
        aria-label={space.name}
        aria-current={space.id === app.activeSpaceId ? "true" : undefined}
      >
        <header class="space-head">
          <span class="space-icon"><SpaceIcon name={space.icon} size={14} /></span>
          <span class="space-name">{space.name}</span>
        </header>

        <div class="space-accounts" role="list">
          {#if loading && accounts.length === 0}
            <p class="empty">Loading contacts…</p>
          {:else if accounts.length === 0}
            <p class="empty">No contact accounts in this space.</p>
          {:else}
            {#each accounts as account (account.id)}
              <div class="account-row" role="listitem">
                <span class="account-symbol"><Users size={15} /></span>
                <span class="account-meta">
                  <strong>{account.displayName}</strong>
                  <span>{account.username} · {account.baseUrl}</span>
                </span>
                <button
                  class="icon-btn danger"
                  type="button"
                  aria-label={`Remove ${account.displayName}`}
                  title="Remove contact account"
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
            Add contact account
          </button>
        </div>
      </section>
    {/each}
  </div>
</section>

{#if showAddFor !== null}
  <ContactAccountDialog
    spaceId={showAddFor}
    onClose={() => (showAddFor = null)}
    onAdded={added}
  />
{/if}

<style>
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
    border: 0;
    border-radius: 0;
    background: transparent;
  }
  .space-card.active {
    padding-left: 9px;
    border-left: 2px solid var(--accent);
  }
  .space-card.active .space-name {
    color: var(--accent);
  }
  .space-card + .space-card {
    padding-top: 14px;
    border-top: 1px solid var(--border);
  }

  .space-head {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 4px 0 8px;
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

  
  .space-accounts {
    display: flex;
    flex-direction: column;
    gap: 0;
    padding: 0;
  }

  .account-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px;
    border: 1px solid transparent;
    border-radius: 8px;
  }

  .account-row {
    border-radius: 0;
  }
  .account-row + .account-row {
    border-top: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  }
  .account-row:hover {
    background: var(--bg-hover);
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
    min-width: 0;
    flex-direction: column;
    gap: 1px;
  }

  .account-meta strong,
  .account-meta span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .account-meta strong {
    color: var(--text);
    font-size: 11px;
    font-weight: 550;
  }

  .account-meta span {
    color: var(--muted);
    font-size: 9px;
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
    opacity: 0.5;
    cursor: default;
  }

  .empty {
    margin: 4px 4px 8px;
    color: var(--muted);
    font-size: 11px;
  }

  .add-account {
    display: inline-flex;
    align-items: center;
    align-self: flex-start;
    gap: 5px;
    margin-top: 2px;
    padding: 5px 7px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--accent);
    font-size: 11px;
    cursor: pointer;
  }

  .add-account:hover {
    background: var(--bg-hover);
  }
</style>
