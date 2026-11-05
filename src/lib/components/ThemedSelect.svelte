<script lang="ts">
  import { Check, ChevronDown } from "lucide-svelte";

  type Option = { value: string; label: string };
  type Group = { label: string; options: Option[] };

  let {
    value,
    options = [],
    groups = null,
    onchange,
    ariaLabel,
    placeholder = "",
    disabled = false,
    separatorAfter = null
  }: {
    value: string | number;
    options?: Option[];
    groups?: Group[] | null;
    onchange: (value: string) => void;
    ariaLabel: string;
    placeholder?: string;
    disabled?: boolean;
    separatorAfter?: string | null;
  } = $props();

  let open = $state(false);
  let root = $state<HTMLDivElement | null>(null);
  const allOptions = $derived(groups ? groups.flatMap((group) => group.options) : options);
  const selected = $derived(allOptions.find((option) => option.value === String(value)));

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
    <span class="value">{selected?.label ?? (placeholder || allOptions[0]?.label || "")}</span>
    <ChevronDown size={13} />
  </button>
  {#if open}
    <div class="menu" role="listbox" aria-label={ariaLabel}>
      {#if groups}
        {#each groups as group (group.label)}
          <div class="group-label" role="presentation">{group.label}</div>
          {#each group.options as option (option.value)}
            {@render optionButton(option)}
          {/each}
        {/each}
      {:else}
        {#each options as option (option.value)}
          {@render optionButton(option)}
          {#if separatorAfter === option.value && options.at(-1)?.value !== option.value}
            <div class="option-divider" role="separator"></div>
          {/if}
        {/each}
      {/if}
    </div>
  {/if}
</div>

{#snippet optionButton(option: Option)}
  <button
    type="button"
    role="option"
    aria-selected={option.value === String(value)}
    onclick={() => choose(option.value)}
  >
    <span>{option.label}</span>
    {#if option.value === String(value)}<Check size={13} />{/if}
  </button>
{/snippet}

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
  .group-label {
    padding: 7px 8px 3px;
    color: var(--muted);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .option-divider {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }
  .menu button[aria-selected="true"] {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--text-strong);
  }
</style>
