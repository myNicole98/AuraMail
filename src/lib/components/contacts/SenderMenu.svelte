<script lang="ts">
  import { onMount } from "svelte";
  import { Copy, Eye, Pencil, Send, UserPlus } from "lucide-svelte";
  import type { ContactRecord } from "../../types";

  let {
    x,
    y,
    name,
    email,
    contact,
    onSend,
    onAdd,
    onEdit,
    onCopy,
    onClose
  }: {
    x: number;
    y: number;
    name: string;
    email: string;
    contact: ContactRecord | null;
    onSend: () => void;
    onAdd: () => void;
    onEdit: () => void;
    onCopy: () => void;
    onClose: () => void;
  } = $props();

  let menu = $state<HTMLElement | null>(null);
  let closed = false;
  const left = $derived(Math.max(8, Math.min(x, window.innerWidth - 216)));
  const top = $derived(Math.max(8, Math.min(y, window.innerHeight - 190)));

  function close(): void {
    if (closed) return;
    closed = true;
    onClose();
  }

  onMount(() => {
    const outside = (event: PointerEvent) => {
      if (!(event.target instanceof Node)) return;
      if (menu?.contains(event.target)) return;
      close();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      close();
    };
    window.addEventListener("pointerdown", outside);
    window.addEventListener("keydown", escape, { capture: true });
    menu?.querySelector<HTMLButtonElement>("button")?.focus();
    return () => {
      window.removeEventListener("pointerdown", outside);
      window.removeEventListener("keydown", escape, { capture: true });
    };
  });
</script>

<div
  class="sender-menu"
  role="menu"
  tabindex="-1"
  bind:this={menu}
  style:left={`${left}px`}
  style:top={`${top}px`}
>
  <div class="sender-meta">
    <strong>{name.trim() || email}</strong>
    <span>{email}</span>
  </div>
  <button type="button" role="menuitem" onclick={onSend}>
    <Send size={14} /> Send message
  </button>
  {#if contact}
    <button type="button" role="menuitem" onclick={onEdit}>
      {#if contact.readOnly}
        <Eye size={14} /> View contact
      {:else}
        <Pencil size={14} /> Edit contact
      {/if}
    </button>
  {:else}
    <button type="button" role="menuitem" onclick={onAdd}>
      <UserPlus size={14} /> Add to contacts
    </button>
  {/if}
  <button type="button" role="menuitem" onclick={onCopy}>
    <Copy size={14} /> Copy address
  </button>
</div>

<style>
  .sender-menu {
    position: fixed;
    z-index: 70;
    display: flex;
    width: 208px;
    flex-direction: column;
    padding: 5px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--bg-raised);
    box-shadow: 0 14px 36px color-mix(in srgb, #000 38%, transparent);
  }

  .sender-menu button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    text-align: left;
    font-size: 12px;
    cursor: pointer;
  }

  .sender-menu button:hover,
  .sender-menu button:focus-visible {
    background: var(--bg-hover);
  }

  .sender-meta {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 1px;
    padding: 5px 8px 7px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    margin-bottom: 4px;
  }

  .sender-meta strong,
  .sender-meta span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sender-meta strong {
    color: var(--text-strong);
    font-size: 12px;
    font-weight: 600;
  }

  .sender-meta span {
    color: var(--muted);
    font-size: 10.5px;
  }
</style>
