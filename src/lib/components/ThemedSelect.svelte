<script lang="ts">
  import { Check, ChevronDown } from "lucide-svelte";

  let {
    value,
    options,
    onchange,
    ariaLabel,
    placeholder = "",
    disabled = false
  }: {
    value: string | number;
    options: { value: string; label: string }[];
    onchange: (value: string) => void;
    ariaLabel: string;
    placeholder?: string;
    disabled?: boolean;
  } = $props();

  let open = $state(false);
  let root = $state<HTMLDivElement | null>(null);
  const selected = $derived(
    options.find((option) => option.value === String(value)) ?? options[0]
  );

  function choose(next: string): void {
    open = false;
    onchange(next);
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      open = false;
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      open = true;
    }
  }
</script>

<div class="themed-select" bind:this={root}>
  <button
    class="trigger"
    type="button"
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-label={ariaLabel}
    {disabled}
    onclick={() => (open = !open)}
    onkeydown={onKeydown}
  >
    <span class="value">{selected?.label ?? placeholder}</span>
    <ChevronDown size={13} />
  </button>
  {#if open}
    <div class="menu" role="listbox" aria-label={ariaLabel}>
      {#each options as option (option.value)}
        <button
          type="button"
          role="option"
          aria-selected={option.value === String(value)}
          onclick={() => choose(option.value)}
        >
          <span>{option.label}</span>
          {#if option.value === String(value)}<Check size={13} />{/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<svelte:window
  onclick={(event) => {
    const target = event.target as Node | null;
    if (root && target && !root.contains(target)) open = false;
  }}
/>

<style>
  .themed-select {
    position: relative;
    min-width: 0;
  }
  .trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    height: 31px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--bg);
    color: var(--text);
    font-size: 11.5px;
    text-align: left;
  }
  .trigger:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
  }
  .trigger:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .trigger:disabled {
    opacity: 0.55;
  }
  .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .menu {
    position: absolute;
    z-index: 120;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    max-height: 230px;
    padding: 4px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--bg-raised);
    box-shadow: var(--pane-shadow, 0 14px 30px rgb(0 0 0 / 28%));
  }
  .menu button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    min-height: 29px;
    padding: 0 8px;
    border-radius: 6px;
    color: var(--text);
    font-size: 11.5px;
    text-align: left;
  }
  .menu button:hover {
    background: var(--bg-hover);
  }
  .menu button[aria-selected="true"] {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--text-strong);
  }
</style>
