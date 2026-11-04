<script lang="ts">
  import { onMount } from "svelte";
  import { CircleAlert, LoaderCircle, Mail, Pencil, Plus, RefreshCw, Search, Trash2, Users } from "lucide-svelte";
  import { api } from "../api";
  import { contactInitials, contactTitle } from "../contacts";
  import { app } from "../stores/app.svelte";
  import type {
    ContactAccount,
    ContactAddressBook,
    ContactRecord
  } from "../types";
  import ContactEditorDialog from "./contacts/ContactEditorDialog.svelte";
  import ThemedSelect from "./ThemedSelect.svelte";

  let records = $state<ContactRecord[]>([]);
  let books = $state<ContactAddressBook[]>([]);
  let accounts = $state<ContactAccount[]>([]);
  let query = $state("");
  let bookId = $state<number | null>(null);
  let selectedId = $state<number | null>(null);
  let loading = $state(false);
  let syncing = $state(false);
  let error = $state<string | null>(null);
  let editor = $state<ContactRecord | "new" | null>(null);
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let loadedSpace: number | null = null;

  const spaceId = $derived(app.activeSpaceId ?? 0);
  const selected = $derived(records.find((record) => record.id === selectedId) ?? null);
  const writable = $derived(selected !== null && !selected.readOnly);

  async function load(keepSelection = true): Promise<void> {
    if (!spaceId) {
      records = [];
      books = [];
      accounts = [];
      return;
    }
    loading = true;
    error = null;
    try {
      const [bookList, recordList, accountList] = await Promise.all([
        api.listAddressBooks(spaceId),
        api.listContactRecords(spaceId, bookId, query || undefined, 500),
        api.listContactAccounts(spaceId)
      ]);
      books = bookList;
      records = recordList;
      accounts = accountList;
      if (!keepSelection || !recordList.some((record) => record.id === selectedId)) {
        selectedId = recordList[0]?.id ?? null;
      }
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading = false;
    }
  }

  function scheduleSearch(): void {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => void load(), 120);
  }

  function chooseBook(value: string): void {
    bookId = value ? Number(value) : null;
    void load();
  }

  async function syncContacts(silent = false): Promise<void> {
    if (syncing || accounts.length === 0) return;
    syncing = true;
    if (!silent) error = null;
    const failures: string[] = [];
    for (const account of accounts) {
      try {
        const result = await api.syncContacts(account.id);
        failures.push(...result.errors);
      } catch (cause) {
        failures.push(cause instanceof Error ? cause.message : String(cause));
      }
    }
    syncing = false;
    localStorage.setItem(`auramail:contacts-synced:${spaceId}`, String(Date.now()));
    await load();
    if (failures.length > 0 && !silent) error = failures[0];
  }

  async function autoSync(): Promise<void> {
    if (accounts.length === 0) return;
    const stored = Number(
      localStorage.getItem(`auramail:contacts-synced:${spaceId}`) ?? 0
    );
    if (Date.now() - stored < 5 * 60 * 1000) return;
    await syncContacts(true);
  }

  async function remove(record: ContactRecord): Promise<void> {
    if (!window.confirm(`Delete ${contactTitle(record)}?`)) return;
    error = null;
    try {
      await api.deleteContact(record.id);
      await load(false);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  function send(record: ContactRecord): void {
    const email = record.emails.find((value) => value.trim());
    if (!email) return;
    void api.openComposeWindow("new", null, null, email);
  }

  function saved(record: ContactRecord): void {
    editor = null;
    void (async () => {
      await load(false);
      selectedId = record.id;
    })();
  }

  $effect(() => {
    const id = app.activeSpaceId;
    if (id && id !== loadedSpace) {
      loadedSpace = id;
      bookId = null;
      selectedId = null;
      void load().then(() => autoSync());
    }
  });

  onMount(() => {
    const refresh = () => {
      void load().then(() => syncContacts(true));
    };
    window.addEventListener("auramail:contacts-changed", refresh);
    return () => window.removeEventListener("auramail:contacts-changed", refresh);
  });
</script>

<section class="contacts-workspace" aria-label="Contacts">
  <aside class="contacts-sidebar">
    <header class="contacts-head">
      <div class="head-title">
        <h2>Contacts</h2>
        <span class="count">{records.length}</span>
      </div>
      <div class="head-actions">
        <button
          class="icon-btn"
          type="button"
          aria-label="Sync contacts"
          title="Sync contacts"
          disabled={syncing || accounts.length === 0}
          onclick={() => void syncContacts()}
        >
          {#if syncing}
            <LoaderCircle class="spin" size={15} />
          {:else}
            <RefreshCw size={15} />
          {/if}
        </button>
        <button class="add-contact" type="button" onclick={() => (editor = "new")}>
          <Plus size={14} /> Add contact
        </button>
      </div>
    </header>

    {#if error}<p class="error" role="alert"><CircleAlert size={13} />{error}</p>{/if}

    <div class="filters">
      <label class="search">
        <Search size={14} />
        <input
          bind:value={query}
          aria-label="Search contacts"
          placeholder="Search contacts"
          oninput={scheduleSearch}
        />
      </label>
      <ThemedSelect
        value={bookId === null ? "" : String(bookId)}
        options={[
          { value: "", label: "All address books" },
          ...books.map((book) => ({ value: String(book.id), label: book.name }))
        ]}
        ariaLabel="Address book filter"
        onchange={chooseBook}
      />
    </div>

    <div class="contact-list" aria-busy={loading}>
      {#if loading && records.length === 0}
        <p class="hint">Loading contacts…</p>
      {:else if records.length === 0}
        <div class="empty">
          <Users size={20} />
          <p>{query ? "No matches." : "No contacts yet."}</p>
        </div>
      {:else}
        {#each records as record (record.id)}
          <button
            class="contact-row"
            type="button"
            class:selected={record.id === selectedId}
            aria-current={record.id === selectedId}
            onclick={() => (selectedId = record.id)}
          >
            {#if record.photo}
              <img class="avatar" src={record.photo} alt="" />
            {:else}
              <span class="avatar initials">
                {contactInitials(record.displayName, record.emails[0] ?? "")}
              </span>
            {/if}
            <span class="row-text">
              <strong>{contactTitle(record)}</strong>
              <span>{record.emails[0] ?? record.phones[0] ?? record.addressBookName}</span>
            </span>
          </button>
        {/each}
      {/if}
    </div>
  </aside>

  <div class="contacts-main">
    {#if selected}
      <article class="contact-detail">
        <header class="detail-head">
          {#if selected.photo}
            <img class="detail-avatar" src={selected.photo} alt="" />
          {:else}
            <span class="detail-avatar initials">
              {contactInitials(selected.displayName, selected.emails[0] ?? "")}
            </span>
          {/if}
          <div class="detail-title">
            <h3>{contactTitle(selected)}</h3>
            <p>{selected.addressBookName}{selected.readOnly ? " · read-only" : ""}</p>
          </div>
          <div class="detail-actions">
            {#if writable}
              <button
                class="secondary"
                type="button"
                onclick={() => (editor = selected)}
              >
                <Pencil size={13} /> Edit
              </button>
              <button class="secondary danger" type="button" onclick={() => void remove(selected)}>
                <Trash2 size={13} /> Delete
              </button>
            {/if}
          </div>
        </header>

        {#if selected.readOnly}
          <p class="read-only-note">This address book is read-only on the server.</p>
        {/if}

        <section class="detail-block">
          <h4>Email</h4>
          {#if selected.emails.length === 0}
            <p class="hint">No email address.</p>
          {:else}
            {#each selected.emails as email (email)}
              <div class="detail-row">
                <span>{email}</span>
                <button
                  class="icon-btn"
                  type="button"
                  aria-label={`Send message to ${email}`}
                  title="Send message"
                  onclick={() => void api.openComposeWindow("new", null, null, email)}
                >
                  <Mail size={14} />
                </button>
              </div>
            {/each}
          {/if}
        </section>

        {#if selected.phones.length > 0}
          <section class="detail-block">
            <h4>Phone</h4>
            {#each selected.phones as phone (phone)}
              <p class="detail-line">{phone}</p>
            {/each}
          </section>
        {/if}

        {#if selected.organization || selected.title}
          <section class="detail-block">
            <h4>Organisation</h4>
            <p class="detail-line">
              {[selected.title, selected.organization].filter(Boolean).join(" · ")}
            </p>
          </section>
        {/if}

        {#if selected.notes}
          <section class="detail-block">
            <h4>Notes</h4>
            <p class="detail-line notes">{selected.notes}</p>
          </section>
        {/if}
      </article>
    {:else}
      <div class="detail-placeholder">
        <Users size={26} />
        <p>{loading ? "Loading contacts…" : "Select a contact"}</p>
      </div>
    {/if}
  </div>
</section>

{#if editor}
  <ContactEditorDialog
    {spaceId}
    contact={editor === "new" ? null : editor}
    onClose={() => (editor = null)}
    onSaved={saved}
  />
{/if}

<style>
  .contacts-workspace {
    display: grid;
    grid-template-columns: 280px minmax(0, 1fr);
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    background: var(--bg);
  }

  .contacts-sidebar {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 10px;
    padding: 14px 12px;
    border-right: 1px solid var(--border);
    background: var(--bg-raised);
  }

  .contacts-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .head-title {
    display: flex;
    align-items: baseline;
    gap: 7px;
  }

  .head-title h2 {
    margin: 0;
    color: var(--text-strong);
    font-size: 13px;
    font-weight: 650;
  }

  .count {
    color: var(--muted);
    font-size: 11px;
  }

  .head-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .add-contact {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 9px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--bg);
    color: var(--text);
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
  }

  .add-contact:hover {
    background: var(--bg-hover);
  }

  .filters {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--bg);
    color: var(--muted);
  }

  .search input {
    width: 100%;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text);
    font-size: 12px;
  }

  .contact-list {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
  }

  .contact-row {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    padding: 7px 8px;
    border: 1px solid transparent;
    border-radius: 8px;
    background: transparent;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }

  .contact-row:hover {
    background: var(--bg-hover);
  }

  .contact-row.selected {
    border-color: color-mix(in srgb, var(--accent) 30%, var(--border));
    background: color-mix(in srgb, var(--accent) 12%, var(--bg-raised));
  }

  .avatar {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    flex: none;
    border-radius: 50%;
    color: var(--text-strong);
    font-size: 11px;
    font-weight: 650;
    object-fit: cover;
  }

  .initials {
    background: color-mix(in srgb, var(--accent) 18%, var(--bg));
  }

  .row-text {
    display: flex;
    min-width: 0;
    flex-direction: column;
  }

  .row-text strong,
  .row-text span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-text strong {
    color: var(--text);
    font-size: 12px;
    font-weight: 600;
  }

  .row-text span {
    color: var(--muted);
    font-size: 11px;
  }

  .hint,
  .empty p {
    color: var(--muted);
    font-size: 12px;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 26px 10px;
    color: var(--muted);
    text-align: center;
  }

  .empty p {
    margin: 0;
  }

  .contacts-main {
    display: flex;
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
  }

  .detail-placeholder {
    display: flex;
    width: 100%;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--muted);
    font-size: 12.5px;
  }

  .contact-detail {
    display: flex;
    width: 100%;
    max-width: 720px;
    flex-direction: column;
    gap: 18px;
    padding: 24px 28px;
  }

  .detail-head {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .detail-avatar {
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    flex: none;
    border-radius: 50%;
    color: var(--text-strong);
    font-size: 16px;
    font-weight: 650;
    object-fit: cover;
  }

  .detail-title {
    min-width: 0;
    flex: 1;
  }

  .detail-title h3 {
    margin: 0;
    overflow: hidden;
    color: var(--text-strong);
    font-size: 18px;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .detail-title p {
    margin: 3px 0 0;
    color: var(--muted);
    font-size: 11.5px;
  }

  .detail-actions {
    display: flex;
    gap: 6px;
  }

  .detail-actions button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: transparent;
    color: var(--text);
    font-size: 11.5px;
    cursor: pointer;
  }

  .detail-actions button:hover {
    background: var(--bg-hover);
  }

  .detail-actions .danger {
    color: var(--danger);
  }

  .detail-block h4 {
    margin: 0 0 6px;
    color: var(--muted);
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .detail-row {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 4px 0;
    color: var(--text);
    font-size: 12.5px;
  }

  .detail-line {
    margin: 0;
    color: var(--text);
    font-size: 12.5px;
    line-height: 1.5;
  }

  .detail-line.notes {
    color: color-mix(in srgb, var(--text) 82%, var(--muted));
    white-space: pre-wrap;
  }

  .read-only-note {
    margin: 0;
    padding: 7px 9px;
    border: 1px solid color-mix(in srgb, var(--accent) 25%, var(--border));
    border-radius: 7px;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    color: var(--muted);
    font-size: 11.5px;
  }

  .error {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--danger);
    font-size: 11.5px;
  }

  .icon-btn {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex: none;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }

  .icon-btn:hover {
    background: var(--bg-hover);
    color: var(--text-strong);
  }

  .icon-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  :global(.spin) {
    animation: spin 900ms linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (max-width: 760px) {
    .contacts-workspace {
      grid-template-columns: 220px minmax(0, 1fr);
    }

    .contact-detail {
      padding: 18px;
    }
  }
</style>
