<script lang="ts">
  import { X } from "lucide-svelte";

  let {
    values,
    onchange,
    suggestions = [],
    placeholder = ""
  }: {
    values: string[];
    onchange: (values: string[]) => void;
    suggestions?: string[];
    placeholder?: string;
  } = $props();

  let text = $state("");
  let focused = $state(false);
  let active = $state(0);

  const matches = $derived(
    text.trim().length === 0
      ? []
      : suggestions
          .filter(
            (suggestion) =>
              !values.includes(suggestion) &&
              suggestion.toLowerCase().includes(text.trim().toLowerCase())
          )
          .slice(0, 7)
  );

  function commit(value: string) {
    const clean = value.trim();
    if (!clean) return;
    if (!values.includes(clean)) onchange([...values, clean]);
    text = "";
    active = 0;
  }

  function remove(index: number) {
    onchange(values.filter((_, i) => i !== index));
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" || event.key === ",") {
      event.preventDefault();
      if (matches.length > 0 && active < matches.length && text.trim().length > 0) {
        commit(matches[active]);
      } else {
        commit(text);
      }
    } else if (event.key === "Backspace" && text === "" && values.length > 0) {
      remove(values.length - 1);
    } else if (event.key === "ArrowDown" && matches.length > 0) {
      event.preventDefault();
      active = (active + 1) % matches.length;
    } else if (event.key === "ArrowUp" && matches.length > 0) {
      event.preventDefault();
      active = (active - 1 + matches.length) % matches.length;
    }
  }
</script>

<div class="tag-input" class:focused>
  {#each values as value, index (value)}
    <span class="chip">
      {value}
      <button
        type="button"
        aria-label={`Remove ${value}`}
        onclick={() => remove(index)}
      >
        <X size={16} />
      </button>
    </span>
  {/each}
  <input
    bind:value={text}
    {placeholder}
    onkeydown={onKeydown}
    onfocus={() => (focused = true)}
    onblur={() => {
      focused = false;
      commit(text);
    }}
  />
  {#if focused && matches.length > 0}
    <div class="suggestions" role="listbox">
      {#each matches as match, index (match)}
        <button
          type="button"
          role="option"
          aria-selected={index === active}
          class:active={index === active}
          onmousedown={(event) => {
            event.preventDefault();
            commit(match);
          }}
        >
          {match}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .tag-input {
    position: relative;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
    flex: 1;
    min-width: 200px;
    min-height: 38px;
    padding: 4px 8px;
    border: 1px solid color-mix(in srgb, var(--border) 85%, transparent);
    border-radius: 7px;
    background: color-mix(in srgb, var(--bg) 65%, transparent);
  }
  .tag-input.focused {
    border-color: color-mix(in srgb, var(--accent) 55%, var(--border));
  }
  .tag-input input {
    flex: 1;
    min-width: 90px;
    /* The settings form styles every input; this one is part of the chip box. */
    height: 24px !important;
    padding: 0 2px !important;
    border: 0 !important;
    background: transparent !important;
    color: var(--text);
    font-size: 12px;
  }
  .tag-input input:focus {
    outline: 0;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 3px 0 9px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--text);
    font-size: 11px;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip button {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    padding: 0;
    line-height: 0;
    border-radius: 5px;
    color: color-mix(in srgb, var(--text) 78%, transparent);
    background: color-mix(in srgb, var(--text) 14%, transparent);
  }
  .chip button:hover {
    color: var(--bg);
    background: var(--danger);
  }
  .suggestions {
    position: absolute;
    z-index: 130;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    max-height: 200px;
    overflow-y: auto;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--bg-raised);
    box-shadow: 0 14px 30px rgb(0 0 0 / 28%);
  }
  .suggestions button {
    display: block;
    width: 100%;
    min-height: 28px;
    padding: 0 8px;
    border-radius: 6px;
    color: var(--text);
    font-size: 11.5px;
    text-align: left;
  }
  .suggestions button:hover,
  .suggestions button.active {
    background: var(--bg-hover);
  }
</style>
