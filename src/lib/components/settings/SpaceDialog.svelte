<script lang="ts">
  import { X } from "lucide-svelte";
  import { SPACE_ICON_NAMES, type SpaceIconName } from "../../spaceIcons";
  import SpaceIcon from "./SpaceIcon.svelte";

  let {
    mode,
    initialName = "",
    initialIcon = "layers",
    onClose,
    onSubmit
  }: {
    mode: "create" | "rename";
    initialName?: string;
    initialIcon?: string;
    onClose: () => void;
    onSubmit: (name: string, icon: string) => Promise<string | null>;
  } = $props();

  // svelte-ignore state_referenced_locally
  let name = $state(initialName);
  // svelte-ignore state_referenced_locally
  let icon = $state<SpaceIconName>(initialIcon as SpaceIconName);
  let error = $state<string | null>(null);
  let saving = $state(false);

  async function submit(): Promise<void> {
    const value = name.trim();
    if (!value) {
      error = "Space name is required.";
      return;
    }
    saving = true;
    const failure = await onSubmit(value, icon);
    saving = false;
    if (failure) error = failure;
    else onClose();
  }
</script>

<div
  class="modal-backdrop"
  role="presentation"
  onclick={(event) => {
    if (event.target === event.currentTarget) onClose();
  }}
>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-label={mode === "create" ? "Create Space" : "Rename space"}
  >
  <form
    class="modal-form"
    onsubmit={(event) => {
      event.preventDefault();
      void submit();
    }}
  >
    <header>
      <h3>{mode === "create" ? "Create Space" : "Rename space"}</h3>
      <button type="button" class="icon-btn" aria-label="Close" onclick={onClose}>
        <X size={15} />
      </button>
    </header>
    <label class="field">
      <span>Space name</span>
      <input
        bind:value={name}
        aria-invalid={Boolean(error)}
        maxlength="40"
        oninput={() => (error = null)}
      />
      {#if error}<small class="error">{error}</small>{/if}
    </label>
    <div class="field">
      <span>Icon</span>
      <div class="icon-grid" role="group" aria-label="Space icon">
        {#each SPACE_ICON_NAMES as option (option)}
          <button
            type="button"
            class="icon-option"
            class:active={icon === option}
            aria-label={`Use ${option} icon`}
            title={option}
            onclick={() => (icon = option)}
          >
            <SpaceIcon name={option} size={16} />
          </button>
        {/each}
      </div>
    </div>
    <footer>
      <button type="button" class="ghost" onclick={onClose}>Cancel</button>
      <button type="submit" class="primary" disabled={saving}>
        {saving ? "Saving…" : mode === "create" ? "Create" : "Rename"}
      </button>
    </footer>
  </form>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    z-index: 80;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 20px;
    background: rgb(0 0 0 / 45%);
  }
  .modal {
    width: min(380px, 100%);
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--bg-raised);
    box-shadow: 0 22px 60px rgb(0 0 0 / 35%);
  }
  .modal-form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  header,
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  h3 {
    margin: 0;
    color: var(--text-strong);
    font-size: 13px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    color: var(--muted);
    font-size: 10.5px;
  }
  input {
    height: 32px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--bg);
    color: var(--text);
    font-size: 12px;
  }
  input[aria-invalid="true"] {
    border-color: var(--danger);
  }
  .error {
    color: var(--danger);
  }
  .icon-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .icon-option {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border: 1px solid var(--border);
    border-radius: 8px;
    color: var(--muted);
  }
  .icon-option:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .icon-option.active {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
  }
  footer {
    justify-content: flex-end;
  }
  .icon-btn {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 6px;
    color: var(--muted);
  }
  .icon-btn:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .ghost,
  .primary {
    height: 30px;
    padding: 0 11px;
    border-radius: 7px;
    font-size: 11.5px;
  }
  .ghost {
    border: 1px solid var(--border);
    color: var(--text);
  }
  .primary {
    background: var(--accent);
    color: var(--bg);
    font-weight: 650;
  }
</style>
