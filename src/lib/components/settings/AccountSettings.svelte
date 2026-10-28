<script lang="ts">
  import { onMount } from "svelte";
  import {
    Check,
    CircleAlert,
    Plus,
    RefreshCw,
    Save,
    Trash2
  } from "lucide-svelte";
  import { accountColorLabel, accountColorTokens, accountColorValue } from "../../accountColors";
  import { api } from "../../api";
  import { validateAccountForm, type AccountForm } from "../../settings";
  import { activeSpaceId, setActiveSpaceId } from "../../spaces";
  import { app } from "../../stores/app.svelte";
  import { appearance } from "../../stores/appearance.svelte";
  import type { Account, AccountInput, SignatureFields, Space } from "../../types";
  import AddAccountDialog from "../account/AddAccountDialog.svelte";
  import AccountEditor from "./AccountEditor.svelte";
  import CalendarAccountSettings from "./CalendarAccountSettings.svelte";
  import DeleteSpaceDialog from "./DeleteSpaceDialog.svelte";
  import SpaceCard from "./SpaceCard.svelte";
  import SpaceDialog from "./SpaceDialog.svelte";
  import { errorMessage } from "./errors";

  let {
    focusAccountId = null
  }: {
    focusAccountId?: number | null;
  } = $props();

  const SAVED_PASSWORD_MASK = "••••••••••••";

  function blankAccount(): AccountForm {
    return {
      display_name: "",
      username: "",
      password: "",
      imap_host: "",
      imap_port: "993",
      imap_security: "tls",
      smtp_host: "",
      smtp_port: "465",
      smtp_security: "tls",
      color: accountColorTokens[app.accounts.length % accountColorTokens.length]
    };
  }

  let showAccountForm = $state(false);
  let accountEditingId = $state<number | null>(null);
  let accountForm = $state<AccountForm>(blankAccount());
  let accountErrors = $state<Record<string, string>>({});
  let accountSaving = $state(false);
  let accountDiscovering = $state(false);
  let accountAdvancedOpen = $state(false);
  let accountDeleting = $state<number | null>(null);
  let accountNotice = $state<string | null>(null);
  let accountError = $state<string | null>(null);
  let dialog = $state<"add" | "reconnect" | null>(null);
  let reconnectAccount = $state<Account | null>(null);
  let appliedFocus = $state<number | null>(null);
  let selectedSpaceId = $state<number | null>(app.activeSpaceId);
  let addingToSpaceId = $state<number | null>(null);
  let collapsedSpaceIds = $state<Record<number, boolean>>({});
  let spaceDialog = $state<
    { mode: "create" } | { mode: "rename"; space: Space } | null
  >(null);
  let deletingSpace = $state<Space | null>(null);

  const spaces = $derived([...app.spaces].sort((a, b) => a.position - b.position));
  const selectedSpace = $derived(
    spaces.find((space) => space.id === selectedSpaceId) ?? null
  );
  const selectedAccounts = $derived(
    app.accounts
      .filter((account) => account.space_id === selectedSpaceId)
      .sort((a, b) => a.position - b.position || a.id - b.id)
  );

  const editingAccount = $derived(
    accountEditingId === null
      ? null
      : (app.accounts.find((account) => account.id === accountEditingId) ?? null)
  );
  const editingOAuth = $derived(editingAccount?.auth_type === "oauth");

  function clearAccountError(field: keyof AccountForm) {
    if (!accountErrors[field]) return;
    const next = { ...accountErrors };
    delete next[field];
    accountErrors = next;
  }

  function openNewAccount(spaceId: number | null = selectedSpaceId ?? app.activeSpaceId) {
    addingToSpaceId = spaceId;
    if (spaceId !== null) selectedSpaceId = spaceId;
    accountEditingId = null;
    accountForm = blankAccount();
    accountErrors = {};
    accountNotice = null;
    accountError = null;
    accountAdvancedOpen = false;
    showAccountForm = true;
  }

  function startManual() {
    dialog = null;
    openNewAccount();
  }

  function openReconnect() {
    if (!editingAccount) return;
    reconnectAccount = editingAccount;
    dialog = "reconnect";
  }

  async function submitOAuthAccount() {
    const account = editingAccount;
    if (!account) return;
    if (!accountForm.display_name.trim()) {
      accountErrors = { display_name: "Display name is required." };
      return;
    }
    accountSaving = true;
    accountNotice = null;
    accountError = null;
    try {
      const input: AccountInput = {
        display_name: accountForm.display_name.trim(),
        email: account.email,
        username: account.username,
        password: "",
        imap_host: account.imap_host,
        imap_port: account.imap_port,
        imap_security: account.imap_security,
        smtp_host: account.smtp_host,
        smtp_port: account.smtp_port,
        smtp_security: account.smtp_security,
        color: accountForm.color,
        signature: account.signature,
        signature_fields: account.signature_fields,
        signature_on_new: account.signature_on_new,
        space_id: account.space_id
      };
      await api.updateAccount(account.id, input);
      await app.refresh();
      accountNotice = "Account updated.";
    } catch (error) {
      accountError = errorMessage(error);
    } finally {
      accountSaving = false;
    }
  }

  async function onOAuthConnected(account: Account) {
    const reconnecting = dialog === "reconnect";
    const provider =
      account.auth_type === "proton"
        ? "Proton"
        : account.oauth_provider === "microsoft"
          ? "Microsoft"
          : "Google";
    dialog = null;
    reconnectAccount = null;
    accountNotice = null;
    accountError = null;
    try {
      await app.refresh();
      if (reconnecting) {
        if (account.auth_type === "proton") {
          app.clearProtonAccessLimited(account.username);
        }
        void app.syncAll();
        accountNotice = `${provider} account reconnected.`;
      } else {
        const fresh = app.accounts.find((candidate) => candidate.id === account.id) ?? account;
        editAccount(fresh);
        void app.syncAll();
        accountNotice =
          account.auth_type === "oauth"
            ? `${provider} account connected.`
            : "Account added.";
      }
    } catch (error) {
      accountError = errorMessage(error);
    }
  }

  function handleColorChanged(account: Account): void {
    app.accounts = app.accounts.map((candidate) =>
      candidate.id === account.id ? account : candidate
    );
  }

  async function handleSaved(account: Account): Promise<void> {
    selectedSpaceId = account.space_id;
    addingToSpaceId = account.space_id;
    await app.refresh();
  }

  async function handleRemoved(): Promise<void> {
    accountEditingId = null;
    await app.refresh();
    accountNotice = "Account removed.";
  }

  function editAccount(account: Account) {
    selectedSpaceId = account.space_id;
    addingToSpaceId = account.space_id;
    accountEditingId = account.id;
    accountForm = {
      display_name: account.display_name,
      username: account.username,
      password: SAVED_PASSWORD_MASK,
      imap_host: account.imap_host,
      imap_port: String(account.imap_port),
      imap_security: account.imap_security,
      smtp_host: account.smtp_host,
      smtp_port: String(account.smtp_port),
      smtp_security: account.smtp_security,
      color: account.color
    };
    accountErrors = {};
    accountNotice = null;
    accountError = null;
    accountAdvancedOpen = false;
    showAccountForm = true;
  }

  function closeAccountForm() {
    showAccountForm = false;
    accountEditingId = null;
    accountErrors = {};
    accountAdvancedOpen = false;
    accountForm = blankAccount();
  }

  async function discoverAccount() {
    if (!accountForm.username.includes("@")) {
      accountError = "Enter an email address first.";
      return;
    }
    accountDiscovering = true;
    accountError = null;
    accountNotice = null;
    try {
      const found = await api.discoverAccountSettings(accountForm.username.trim());
      accountForm = {
        ...accountForm,
        imap_host: found.imap_host,
        imap_port: String(found.imap_port),
        imap_security: found.imap_security,
        smtp_host: found.smtp_host,
        smtp_port: String(found.smtp_port),
        smtp_security: found.smtp_security
      };
      accountErrors = {};
      accountNotice =
        found.source === "guess"
          ? "Settings guessed. Verify them before saving."
          : "Mail settings detected.";
    } catch (error) {
      accountError = errorMessage(error);
    } finally {
      accountDiscovering = false;
    }
  }

  async function submitAccount() {
    const editing = accountEditingId !== null;
    const errors = validateAccountForm(accountForm, !editing);
    accountErrors = errors;
    accountNotice = null;
    accountError = null;
    if (
      errors.imap_host ||
      errors.imap_port ||
      errors.imap_security ||
      errors.smtp_host ||
      errors.smtp_port ||
      errors.smtp_security
    ) {
      accountAdvancedOpen = true;
    }
    if (Object.keys(errors).length > 0) return;
    accountSaving = true;
    try {
      const input: AccountInput = {
        display_name: accountForm.display_name.trim(),
        email: accountForm.username.trim(),
        username: accountForm.username.trim(),
        password:
          accountForm.password === SAVED_PASSWORD_MASK ? "" : accountForm.password,
        imap_host: accountForm.imap_host.trim(),
        imap_port: Number(accountForm.imap_port),
        imap_security: accountForm.imap_security,
        smtp_host: accountForm.smtp_host.trim(),
        smtp_port: Number(accountForm.smtp_port),
        smtp_security: accountForm.smtp_security,
        color: accountForm.color,
        signature:
          accountEditingId === null
            ? ""
            : (app.accounts.find((account) => account.id === accountEditingId)
                ?.signature ?? ""),
        signature_fields:
          accountEditingId === null
            ? blankSignatureFields()
            : (app.accounts.find((account) => account.id === accountEditingId)
                ?.signature_fields ?? blankSignatureFields()),
        signature_on_new:
          accountEditingId === null
            ? true
            : (app.accounts.find((account) => account.id === accountEditingId)
                ?.signature_on_new ?? true),
        space_id:
          accountEditingId === null
            ? (addingToSpaceId ?? selectedSpaceId ?? activeSpaceId() ?? 1)
            : (app.accounts.find((account) => account.id === accountEditingId)
                ?.space_id ?? selectedSpaceId ?? activeSpaceId() ?? 1)
      };
      if (editing) {
        await api.updateAccount(accountEditingId!, input);
      } else {
        await api.addAccount(input);
      }
      await app.refresh();
      accountForm = blankAccount();
      showAccountForm = false;
      accountEditingId = null;
      accountNotice = editing ? "Account updated and verified." : "Account added and verified.";
    } catch (error) {
      accountError = errorMessage(error);
    } finally {
      accountSaving = false;
    }
  }

  async function removeAccount(account: Account) {
    if (!window.confirm(`Remove ${account.email} and its cached mail?`)) return;
    accountDeleting = account.id;
    accountNotice = null;
    accountError = null;
    try {
      await api.deleteAccount(account.id);
      app.clearProtonAccessLimited(account.username);
      if (accountEditingId === account.id) {
        showAccountForm = false;
        accountEditingId = null;
        accountForm = blankAccount();
      }
      await app.refresh();
      accountNotice = "Account removed.";
    } catch (error) {
      accountError = errorMessage(error);
    } finally {
      accountDeleting = null;
    }
  }

  function selectSpace(space: Space): void {
    selectedSpaceId = space.id;
    accountNotice = null;
    accountError = null;
    if (accountEditingId !== null) {
      const editing = app.accounts.find((account) => account.id === accountEditingId);
      if (editing?.space_id !== space.id) {
        showAccountForm = false;
        accountEditingId = null;
        accountForm = blankAccount();
      }
    }
  }

  async function createSpace(name: string, icon: string): Promise<string | null> {
    try {
      const created = await api.createSpace(name, icon);
      app.spaces = [...app.spaces, created];
      selectedSpaceId = created.id;
      addingToSpaceId = created.id;
      accountEditingId = null;
      accountNotice = `Created ${created.name}.`;
      return null;
    } catch (error) {
      return errorMessage(error);
    }
  }

  async function renameSpace(space: Space, name: string, icon: string): Promise<string | null> {
    try {
      const renamed = await api.renameSpace(space.id, name, icon);
      app.spaces = app.spaces.map((candidate) =>
        candidate.id === renamed.id ? renamed : candidate
      );
      return null;
    } catch (error) {
      return errorMessage(error);
    }
  }

  async function confirmDeleteSpace(
    action:
      | { kind: "delete_accounts" }
      | { kind: "migrate"; targetSpaceId: number }
  ): Promise<string | null> {
    const deleting = deletingSpace;
    if (!deleting) return null;
    try {
      const remaining = await api.deleteSpace(deleting.id, action);
      const wasActive = app.activeSpaceId === deleting.id;
      app.spaces = remaining;
      if (wasActive) {
        const next = remaining.find(
          (space) => action.kind === "migrate" && space.id === action.targetSpaceId
        ) ?? remaining[0];
        if (next) {
          await app.switchSpace(next.id);
          if (app.activeSpaceId !== next.id) {
            app.activeSpaceId = next.id;
            setActiveSpaceId(next.id);
          }
        }
        selectedSpaceId = next?.id ?? null;
      } else {
        selectedSpaceId =
          action.kind === "migrate" ? action.targetSpaceId : (remaining[0]?.id ?? null);
        await app.refresh();
      }
      accountNotice = `Deleted ${deleting.name}.`;
      return null;
    } catch (error) {
      return errorMessage(error);
    }
  }

  let draggedAccountId = $state<number | null>(null);
  let dragTargetSpaceId = $state<number | null>(null);

  function startDragging(accountId: number): void {
    draggedAccountId = accountId;
    dragTargetSpaceId = null;
  }

  function dragOverSpace(spaceId: number): void {
    if (draggedAccountId === null) return;
    const dragged = app.accounts.find(
      (account) => account.id === draggedAccountId
    );
    dragTargetSpaceId =
      dragged && dragged.space_id !== spaceId ? spaceId : null;
  }

  function dragOverAccount(spaceId: number, targetId: number, before: boolean): void {
    const draggedId = draggedAccountId;
    if (draggedId === null || draggedId === targetId) return;
    const dragged = app.accounts.find((account) => account.id === draggedId);
    if (!dragged || dragged.space_id !== spaceId) return;
    const accounts = orderedAccounts(spaceId);
    const remaining = accounts.filter((account) => account.id !== draggedId);
    const targetIndex = remaining.findIndex((account) => account.id === targetId);
    if (targetIndex < 0) return;
    remaining.splice(before ? targetIndex : targetIndex + 1, 0, dragged);
    if (remaining.every((account, index) => accounts[index]?.id === account.id)) {
      return;
    }
    const updated = remaining.map((account, index) => ({
      ...account,
      position: index
    }));
    app.accounts = [
      ...app.accounts.filter((account) => account.space_id !== spaceId),
      ...updated
    ];
  }

  async function persistAccountOrder(spaceId: number): Promise<void> {
    const accounts = orderedAccounts(spaceId);
    try {
      const reordered = await api.reorderAccounts(
        spaceId,
        accounts.map((account) => account.id)
      );
      app.accounts = [
        ...app.accounts.filter((account) => account.space_id !== spaceId),
        ...reordered
      ];
      selectedSpaceId = spaceId;
      accountError = null;
    } catch (error) {
      accountError = errorMessage(error);
    }
  }

  function orderedAccounts(spaceId: number): Account[] {
    return app.accounts
      .filter((account) => account.space_id === spaceId)
      .sort((a, b) => a.position - b.position || a.id - b.id);
  }

  async function dropOnAccount(spaceId: number, targetId: number): Promise<void> {
    const draggedId = draggedAccountId;
    draggedAccountId = null;
    dragTargetSpaceId = null;
    if (draggedId === null) return;
    const dragged = app.accounts.find((account) => account.id === draggedId);
    if (!dragged) return;
    if (dragged.space_id === spaceId) {
      await persistAccountOrder(spaceId);
      return;
    }
    try {
      const movedAccount = await api.moveAccount(draggedId, spaceId);
      app.accounts = app.accounts.map((account) =>
        account.id === draggedId ? movedAccount : account
      );
      const accounts = orderedAccounts(spaceId);
      const targetIndex = accounts.findIndex((account) => account.id === targetId);
      const sourceIndex = accounts.findIndex((account) => account.id === draggedId);
      if (sourceIndex >= 0 && targetIndex >= 0) {
        const [moved] = accounts.splice(sourceIndex, 1);
        accounts.splice(targetIndex, 0, moved);
      }
      app.accounts = [
        ...app.accounts.filter((account) => account.space_id !== spaceId),
        ...accounts.map((account, index) => ({ ...account, position: index }))
      ];
      await persistAccountOrder(spaceId);
    } catch (error) {
      accountError = errorMessage(error);
    }
  }

  async function dropOnSpace(spaceId: number): Promise<void> {
    const draggedId = draggedAccountId;
    draggedAccountId = null;
    dragTargetSpaceId = null;
    if (draggedId === null) return;
    const dragged = app.accounts.find((account) => account.id === draggedId);
    if (!dragged) return;
    if (dragged.space_id === spaceId) {
      await persistAccountOrder(spaceId);
      return;
    }
    try {
      const movedAccount = await api.moveAccount(draggedId, spaceId);
      app.accounts = app.accounts.map((account) =>
        account.id === draggedId ? movedAccount : account
      );
      selectedSpaceId = spaceId;
      accountError = null;
    } catch (error) {
      accountError = errorMessage(error);
    }
  }

  async function moveAccount(space: Space, account: Account, direction: -1 | 1): Promise<void> {
    const accounts = app.accounts
      .filter((candidate) => candidate.space_id === space.id)
      .sort((a, b) => a.position - b.position || a.id - b.id);
    const index = accounts.findIndex((candidate) => candidate.id === account.id);
    const target = index + direction;
    if (index < 0 || target < 0 || target >= accounts.length) return;
    [accounts[index], accounts[target]] = [accounts[target], accounts[index]];
    try {
      const reordered = await api.reorderAccounts(
        space.id,
        accounts.map((candidate) => candidate.id)
      );
      app.accounts = [
        ...app.accounts.filter((candidate) => candidate.space_id !== space.id),
        ...reordered
      ];
      accountError = null;
    } catch (error) {
      accountError = errorMessage(error);
    }
  }

  $effect(() => {
    const id = focusAccountId ?? null;
    if (id === null || appliedFocus === id) return;
    const account = app.accounts.find((candidate) => candidate.id === id);
    if (!account) return;
    appliedFocus = id;
    editAccount(account);
  });

  onMount(() => {
    if (selectedSpaceId === null) selectedSpaceId = app.activeSpaceId ?? spaces[0]?.id ?? null;
    const focused =
      focusAccountId == null
        ? null
        : app.accounts.find((account) => account.id === focusAccountId);
    if (focused) {
      editAccount(focused);
      return;
    }
    const accounts = app.accounts.filter(
      (account) => account.space_id === selectedSpaceId
    );
    if (accounts.length === 0) {
      if (app.accounts.length === 0 && focusAccountId == null) dialog = "add";
      return;
    }
    editAccount(accounts[0]);
  });

  function focusPassword() {
    if (accountForm.password === SAVED_PASSWORD_MASK) {
      accountForm.password = "";
    }
  }

  function blurPassword() {
    if (accountEditingId !== null && accountForm.password === "") {
      accountForm.password = SAVED_PASSWORD_MASK;
    }
  }

  function blankSignatureFields(): SignatureFields {
    return {
      profession: "",
      phone: "",
      location: "",
      website: "",
      socials: []
    };
  }
</script>

<div class="settings-inner">
  {#if accountNotice}
    <p class="notice" role="status"><Check size={14} />{accountNotice}</p>
  {/if}
  {#if accountError}
    <p class="alert" role="alert"><CircleAlert size={14} />{accountError}</p>
  {/if}

  <div class="account-layout">
    <aside class="account-rail" aria-label="Spaces">
      <div class="space-list">
        {#each spaces as space (space.id)}
          <SpaceCard
            {space}
            accounts={app.accounts
              .filter((account) => account.space_id === space.id)
              .sort((a, b) => a.position - b.position || a.id - b.id)}
            active={space.id === app.activeSpaceId}
            selectedAccountId={accountEditingId}
            unread={app.spaceUnreadCounts[space.id] ?? 0}
            draggedAccountId={draggedAccountId}
            dragTarget={dragTargetSpaceId === space.id}
            collapsed={collapsedSpaceIds[space.id] === true}
            onToggle={() => {
              selectSpace(space);
              collapsedSpaceIds = {
                ...collapsedSpaceIds,
                [space.id]: !collapsedSpaceIds[space.id]
              };
            }}
            onAddAccount={() => {
              addingToSpaceId = space.id;
              selectedSpaceId = space.id;
              dialog = "add";
            }}
            onSelectAccount={editAccount}
            onRename={() => (spaceDialog = { mode: "rename", space })}
            onDelete={() => {
              if (app.spaces.length <= 1) {
                accountError = "The last space cannot be deleted.";
              } else {
                deletingSpace = space;
              }
            }}
            onMove={(account, direction) => void moveAccount(space, account, direction)}
            onDragStart={startDragging}
            onDragEnd={() => {
              draggedAccountId = null;
              dragTargetSpaceId = null;
            }}
            onDragOverSpace={() => dragOverSpace(space.id)}
            onDragLeaveSpace={() => {
              if (dragTargetSpaceId === space.id) dragTargetSpaceId = null;
            }}
            onDragOverAccount={(targetId, before) =>
              dragOverAccount(space.id, targetId, before)}
            onDropSpace={() => void dropOnSpace(space.id)}
            onDropAccount={(targetId) => void dropOnAccount(space.id, targetId)}
          />
        {/each}
      </div>
      <button
        class="create-space"
        type="button"
        onclick={() => (spaceDialog = { mode: "create" })}
      >
        <Plus size={14} />
        Create Space
      </button>
    </aside>

    <div class="account-detail">
      {#if editingAccount}
        {#key `${editingAccount.id}:${editingAccount.space_id}`}
          <AccountEditor
            account={editingAccount}
            {spaces}
            onSaved={handleSaved}
            onRemoved={handleRemoved}
            onColorChanged={handleColorChanged}
            onReconnect={openReconnect}
          />
        {/key}
      {/if}
    </div>
  </div>

  <CalendarAccountSettings />

  {#if dialog}
    <AddAccountDialog
      mode={dialog}
      account={dialog === "reconnect" ? reconnectAccount : null}
      spaceId={addingToSpaceId ?? selectedSpaceId ?? app.activeSpaceId ?? undefined}
      onClose={() => (dialog = null)}
      onConnected={onOAuthConnected}
    />
  {/if}

  {#if spaceDialog}
    <SpaceDialog
      mode={spaceDialog.mode}
      initialName={spaceDialog.mode === "rename" ? spaceDialog.space.name : ""}
      initialIcon={spaceDialog.mode === "rename" ? spaceDialog.space.icon : "layers"}
      onClose={() => (spaceDialog = null)}
      onSubmit={(name, icon) =>
        spaceDialog?.mode === "rename"
          ? renameSpace(spaceDialog.space, name, icon)
          : createSpace(name, icon)}
    />
  {/if}

  {#if deletingSpace}
    <DeleteSpaceDialog
      space={deletingSpace}
      {spaces}
      onClose={() => (deletingSpace = null)}
      onConfirm={confirmDeleteSpace}
    />
  {/if}
</div>

<style>
  .account-layout {
    display: grid;
    grid-template-columns: minmax(240px, 0.68fr) minmax(0, 1.32fr);
    gap: 22px;
    align-items: start;
  }
  .account-rail {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .space-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .create-space {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    width: 100%;
    height: 34px;
    margin-top: 12px;
    border: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
    border-radius: 9px;
    color: var(--muted);
    font-size: 11px;
  }
  .create-space:hover {
    border-color: var(--accent);
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 6%, transparent);
  }
  .account-detail {
    min-width: 0;
  }
  @media (max-width: 820px) {
    .account-layout {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
