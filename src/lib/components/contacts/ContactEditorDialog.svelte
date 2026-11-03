<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { LoaderCircle, Plus, X } from "lucide-svelte";
  import { api } from "../../api";
  import type { ContactAddressBook, ContactInput, ContactRecord } from "../../types";
  import ThemedSelect from "../ThemedSelect.svelte";
  import { validEmails } from "../../contacts";

  let {
    spaceId,
    contact = null,
    seed = null,
    onClose,
    onSaved
  }: {
    spaceId: number;
    contact?: ContactRecord | null;
    seed?: { name: string; email: string } | null;
    onClose: () => void;
    onSaved: (record: ContactRecord) => void;
  } = $props();

  let name = $state(untrack(() => contact?.displayName ?? seed?.name ?? ""));
  let emails = $state<string[]>(
    untrack(() =>
      contact?.emails?.length ? [...contact.emails] : seed?.email ? [seed.email] : [""]
    )
  );
  let phones = $state<string[]>(
    untrack(() => (contact?.phones?.length ? [...contact.phones] : [""]))
  );
  let organization = $state(untrack(() => contact?.organization ?? ""));
  let title = $state(untrack(() => contact?.title ?? ""));
  let notes = $state(untrack(() => contact?.notes ?? ""));
  let books = $state<ContactAddressBook[]>([]);
  let bookId = $state<number | null>(null);
  let saving = $state(false);
  let error = $state<string | null>(null);

  const editing = untrack(() => contact !== null);
  const writableBooks = $derived(books.filter((book) => !book.readOnly));

  onMount(() => {
    if (editing) return;
    void api
      .listAddressBooks(spaceId)
      .then((result) => {
        books = result;
        const local = result.find((book) => book.href === "local" && !book.readOnly);
        bookId = local?.id ?? result.find((book) => !book.readOnly)?.id ?? null;
      })
      .catch((cause) => {
        error = cause instanceof Error ? cause.message : String(cause);
      });
  });

  function addEmail(): void {
    emails = [...emails, ""];
  }

  function removeEmail(index: number): void {
    emails = emails.length > 1 ? emails.filter((_, position) => position !== index) : [""];
  }

  function addPhone(): void {
    phones = [...phones, ""];
  }

  function removePhone(index: number): void {
    phones = phones.length > 1 ? phones.filter((_, position) => position !== index) : [""];
  }

  async function save(): Promise<void> {
    if (saving) return;
    const cleanEmails = emails.map((email) => email.trim()).filter(Boolean);
    if (!name.trim() && cleanEmails.length === 0) {
      error = "Add a name or at least one email address.";
      return;
    }
    if (!validEmails(emails)) {
      error = "One of the email addresses is not valid.";
      return;
    }
    if (!editing && bookId === null) {
      error = "Choose an address book.";
      return;
    }
    const input: ContactInput = {
      displayName: name.trim(),
      emails: cleanEmails,
      phones: phones.map((phone) => phone.trim()).filter(Boolean),
      organization: organization.trim(),
      title: title.trim(),
      notes: notes.trim(),
      addressBookId: editing ? null : bookId
    };
    saving = true;
    error = null;
    try {
      const saved = editing
        ? await api.updateContact(contact!.id, input)
        : await api.createContact(input);
      onSaved(saved);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      saving = false;
    }
  }
</script>

<div
  class="dialog-layer"
  role="presentation"
  onclick={(event) => {
    if (event.target === event.currentTarget) onClose();
  }}
  onkeydown={(event) => {
    if (event.key === "Escape") onClose();
  }}
>
  <div
    class="contact-dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="contact-dialog-title"
  >
    <form
      class="dialog-form"
      onsubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
    <header class="dialog-head">
      <h3 id="contact-dialog-title">{editing ? "Edit contact" : "New contact"}</h3>
      <button class="icon-btn" type="button" aria-label="Close" onclick={onClose}>
        <X size={15} />
      </button>
    </header>

    {#if error}<p class="dialog-error" role="alert">{error}</p>{/if}

    <div class="dialog-grid">
      <label class="field">
        <span>Name</span>
        <input bind:value={name} autocomplete="name" />
      </label>
      <label class="field">
        <span>Organisation</span>
        <input bind:value={organization} autocomplete="organization" />
      </label>
      <label class="field">
        <span>Title</span>
        <input bind:value={title} autocomplete="organization-title" />
      </label>
      {#if editing}
        <div class="field">
          <span>Address book</span>
          <p class="static-value">{contact!.addressBookName}</p>
        </div>
      {:else}
        <div class="field">
          <span>Address book</span>
          <ThemedSelect
            value={bookId ?? ""}
            options={writableBooks.map((book) => ({
              value: String(book.id),
              label: book.name
            }))}
            ariaLabel="Address book"
            onchange={(value) => (bookId = Number(value))}
          />
        </div>
      {/if}
    </div>

    <fieldset class="value-group">
      <legend>Email</legend>
      {#each emails as email, index (index)}
        <div class="value-row">
          <input
            bind:value={emails[index]}
            aria-label={`Email ${index + 1}`}
            type="text"
            inputmode="email"
            autocomplete="email"
          />
          <button
            class="icon-btn"
            type="button"
            aria-label={`Remove email ${index + 1}`}
            onclick={() => removeEmail(index)}
          >
            <X size={13} />
          </button>
        </div>
      {/each}
      <button class="add-value" type="button" onclick={addEmail}>
        <Plus size={13} /> Add email
      </button>
    </fieldset>

    <fieldset class="value-group">
      <legend>Phone</legend>
      {#each phones as phone, index (index)}
        <div class="value-row">
          <input
            bind:value={phones[index]}
            aria-label={`Phone ${index + 1}`}
            type="tel"
            autocomplete="tel"
          />
          <button
            class="icon-btn"
            type="button"
            aria-label={`Remove phone ${index + 1}`}
            onclick={() => removePhone(index)}
          >
            <X size={13} />
          </button>
        </div>
      {/each}
      <button class="add-value" type="button" onclick={addPhone}>
        <Plus size={13} /> Add phone
      </button>
    </fieldset>

    <label class="field">
      <span>Notes</span>
      <textarea bind:value={notes} rows="3"></textarea>
    </label>

    <footer class="dialog-actions">
      <button class="secondary" type="button" onclick={onClose}>Cancel</button>
      <button class="primary" type="submit" disabled={saving}>
        {#if saving}<LoaderCircle class="spin" size={14} />{/if}
        {editing ? "Save contact" : "Add contact"}
      </button>
      </footer>
    </form>
  </div>
</div>

<style>
  .dialog-layer {
    position: fixed;
    z-index: 80;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 24px;
    background: color-mix(in srgb, #000 55%, transparent);
  }

  .contact-dialog {
    display: flex;
    width: min(560px, 100%);
    max-height: calc(100vh - 48px);
    flex-direction: column;
    gap: 14px;
    padding: 18px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--bg-raised);
    box-shadow: 0 24px 60px color-mix(in srgb, #000 45%, transparent);
  }

  .dialog-form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .dialog-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .dialog-head h3 {
    margin: 0;
    color: var(--text-strong);
    font-size: 15px;
    font-weight: 650;
  }

  .dialog-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  .field {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 5px;
    color: var(--muted);
    font-size: 12px;
  }

  .field input,
  .field textarea,
  .value-row input {
    width: 100%;
    padding: 8px 9px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--bg);
    color: var(--text);
    font-size: 12.5px;
    font-family: inherit;
  }

  .value-group {
    display: flex;
    flex-direction: column;
    gap: 7px;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .value-group legend {
    margin-bottom: 3px;
    color: var(--muted);
    font-size: 12px;
  }

  .value-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .add-value {
    display: inline-flex;
    align-items: center;
    align-self: flex-start;
    gap: 5px;
    padding: 4px 6px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--accent);
    font-size: 11.5px;
    cursor: pointer;
  }

  .add-value:hover {
    background: var(--bg-hover);
  }

  .static-value {
    margin: 0;
    padding: 8px 9px;
    border: 1px solid var(--border);
    border-radius: 7px;
    color: var(--text);
    font-size: 12.5px;
  }

  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 2px;
  }

  .dialog-actions button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border-radius: 7px;
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
  }

  .dialog-actions .secondary {
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text);
  }

  .dialog-actions .primary {
    border: 1px solid transparent;
    background: var(--accent);
    color: var(--accent-contrast, #fff);
  }

  .dialog-actions button:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .icon-btn {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
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

  .dialog-error {
    margin: 0;
    color: var(--danger);
    font-size: 12px;
  }

  :global(.spin) {
    animation: spin 900ms linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
