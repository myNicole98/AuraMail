<script lang="ts">
  let {
    value,
    options,
    ariaLabel,
    onchange
  }: {
    value: string;
    options: { value: string; label: string }[];
    ariaLabel: string;
    onchange: (value: string) => void;
  } = $props();
</script>

<div class="segmented" role="group" aria-label={ariaLabel}>
  {#each options as option (option.value)}
    <button
      type="button"
      aria-pressed={option.value === value}
      onclick={() => onchange(option.value)}
    >
      {option.label}
    </button>
  {/each}
</div>

<style>
  .segmented {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border-radius: 7px;
    background: color-mix(in srgb, var(--bg-hover) 55%, transparent);
  }

  .segmented button {
    min-height: 28px;
    padding: 0 9px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
    font-size: 11.5px;
    font-weight: 550;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background 120ms ease,
      color 120ms ease;
  }

  .segmented button:hover {
    color: var(--text);
    background: var(--bg-hover);
  }

  .segmented button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .segmented button[aria-pressed="true"] {
    background: var(--accent);
    color: var(--bg);
  }
</style>
