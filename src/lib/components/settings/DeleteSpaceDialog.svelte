<script lang="ts">
  import { X } from "lucide-svelte";
  import ThemedSelect from "../ThemedSelect.svelte";
  import type { Space } from "../../types";

  let {
    space,
    spaces,
    onClose,
    onConfirm
  }: {
    space: Space;
    spaces: Space[];
    onClose: () => void;
    onConfirm: (
      action:
        | { kind: "delete_accounts" }
        | { kind: "migrate"; targetSpaceId: number }
    ) => Promise<string | null>;
  } = $props();

  const targets = $derived(spaces.filter((candidate) => candidate.id !== space.id));
  let mode = $state<"delete" | "migrate">("migrate");
  let targetId = $state<number>(0);
  $effect(() => {
    if (targetId === 0 && targets.length > 0) targetId = targets[0].id;
  });
  let error = $state<string | null>(null);
  let saving = $state(false);

  async function submit(): Promise<void> {
    if (mode === "migrate" && targetId === 0) {
      error = "Choose a space for the accounts.";
      return;
    }
    saving = true;
    const failure = await onConfirm(
      mode === "migrate"
        ? { kind: "migrate", targetSpaceId: targetId }
        : { kind: "delete_accounts" }
    );
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
  <div class="modal" role="dialog" aria-modal="true" aria-label="Delete space">
    <header>
      <h3>Delete {space.name}</h3>
      <button type="button" class="icon-btn" aria-label="Close" onclick={onClose}>
        <X size={15} />
      </button>
    </header>
    <p class="explain">
      This space's tabs, appearance, and AI settings are removed. Choose what happens to its accounts.
    </p>
    {#if targets.length > 0}
      <label class="radio">
        <input type="radio" bind:group={mode} value="migrate" />
        <span>Move accounts to another space</span>
      </label>
      {#if mode === "migrate"}
        <ThemedSelect
          value={targetId}
          options={targets.map((target) => ({
            value: String(target.id),
            label: target.name
          }))}
          ariaLabel="Destination space"
          onchange={(value) => (targetId = Number(value))}
        />
      {/if}
    {/if}
    <label class="radio">
      <input type="radio" bind:group={mode} value="delete" />
      <span>Delete its accounts and cached mail</span>
    </label>
    {#if error}<small class="error">{error}</small>{/if}
    <footer>
      <button type="button" class="ghost" onclick={onClose}>Cancel</button>
      <button type="button" class="danger" disabled={saving} onclick={() => void submit()}>
        {saving ? "Deleting…" : "Delete space"}
      </button>
    </footer>
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
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: min(420px, 100%);
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--bg-raised);
    box-shadow: 0 22px 60px rgb(0 0 0 / 35%);
  }
  header,
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  footer {
    justify-content: flex-end;
  }
  h3 {
    margin: 0;
    color: var(--text-strong);
    font-size: 13px;
  }
  .explain {
    margin: 0;
    color: var(--muted);
    font-size: 11px;
    line-height: 1.45;
  }
  .radio {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text);
    font-size: 11.5px;
  }
  .error {
    color: var(--danger);
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
  .danger {
    height: 30px;
    padding: 0 11px;
    border-radius: 7px;
    font-size: 11.5px;
  }
  .ghost {
    border: 1px solid var(--border);
    color: var(--text);
  }
  .danger {
    background: var(--danger);
    color: white;
    font-weight: 650;
  }
</style>
