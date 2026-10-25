<script lang="ts">
  import { Check, ChevronDown, RefreshCw } from "lucide-svelte";
  import { modelIconUrl } from "../../ai/providers";

  let {
    value = $bindable(""),
    models = [],
    loading = false,
    error = null,
    label = "Model",
    placeholder = "model name",
    refreshLabel = "Refresh models",
    onload,
    onrefresh
  }: {
    value?: string;
    models?: string[];
    loading?: boolean;
    error?: string | null;
    label?: string;
    placeholder?: string;
    refreshLabel?: string;
    onload?: () => void;
    onrefresh?: () => void;
  } = $props();

  let open = $state(false);
  let icon = $derived(modelIconUrl(value));
  let hasMenu = $derived(models.length > 0);

  function choose(model: string) {
    value = model;
    open = false;
  }
</script>

<div class="model-field">
  <div class="model-row">
    <div class="provider-select">
      {#if hasMenu}
        <button
          class="provider-trigger"
          class:open
          type="button"
          aria-label={label}
          aria-haspopup="listbox"
          aria-expanded={open}
          onclick={() => {
            open = !open;
            onload?.();
          }}
        >
          {#if icon}<img class="model-icon" src={icon} alt="" />{/if}
          <span class="provider-label">{value || "Select model"}</span>
          <ChevronDown size={14} />
        </button>
        {#if open}
          <button
            class="provider-backdrop"
            type="button"
            aria-label={`Close ${label.toLowerCase()} menu`}
            onclick={() => (open = false)}
          ></button>
          <div
            class="provider-menu"
            role="listbox"
            aria-label={label}
            tabindex="-1"
            onkeydown={(event) => {
              if (event.key === "Escape") {
                open = false;
                event.stopPropagation();
              }
            }}
          >
            {#if value && !models.includes(value)}
              <button
                type="button"
                class="active"
                role="option"
                aria-selected="true"
                onclick={() => (open = false)}
              >
                <span class="provider-label">{value}</span>
                <Check size={14} />
              </button>
            {/if}
            {#each models as model (model)}
              <button
                type="button"
                class:active={value === model}
                role="option"
                aria-selected={value === model}
                onclick={() => choose(model)}
              >
                <span class="provider-label">{model}</span>
                {#if value === model}<Check size={14} />{/if}
              </button>
            {/each}
          </div>
        {/if}
      {:else}
        <div class="input-with-icon">
          {#if icon}<img class="model-icon" src={icon} alt="" />{/if}
          <input
            bind:value
            aria-label={label}
            placeholder={placeholder}
            autocomplete="off"
            class:has-icon={Boolean(icon)}
            onfocus={() => onload?.()}
          />
        </div>
      {/if}
    </div>
    {#if onrefresh}
      <button
        class="refresh-models"
        type="button"
        aria-label={refreshLabel}
        title={refreshLabel}
        disabled={loading}
        onclick={() => onrefresh()}
      >
        <span class:spin={loading}><RefreshCw size={13} /></span>
      </button>
    {/if}
  </div>
  {#if loading}
    <small class="model-help">Loading models…</small>
  {:else if error}
    <small class="model-error">{error}</small>
  {/if}
</div>

<style>
  .model-field {
    min-width: 0;
  }
  .model-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .provider-select {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  .provider-trigger {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    min-height: 31px;
    padding: 0 9px;
    border: 1px solid color-mix(in srgb, var(--border) 78%, transparent);
    border-radius: 7px;
    background: color-mix(in srgb, var(--bg) 55%, var(--bg-raised));
    color: var(--text);
    font-size: 10px;
    text-align: left;
  }
  .provider-trigger :global(svg) {
    margin-left: auto;
    flex: none;
    color: var(--muted);
    transition: transform 140ms ease;
  }
  .provider-trigger.open :global(svg) {
    transform: rotate(180deg);
  }
  .provider-trigger:hover,
  .provider-trigger:focus-visible {
    border-color: color-mix(in srgb, var(--accent) 35%, var(--border));
    background: color-mix(in srgb, var(--bg-hover) 45%, var(--bg-raised));
  }
  .provider-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .input-with-icon {
    position: relative;
  }
  .input-with-icon input {
    width: 100%;
    min-height: 31px;
    padding: 6px 9px;
    border: 1px solid color-mix(in srgb, var(--border) 78%, transparent);
    border-radius: 7px;
    background-color: color-mix(in srgb, var(--bg) 55%, var(--bg-raised));
    color: var(--text);
    font-size: 10px;
  }
  .input-with-icon input.has-icon {
    padding-left: 30px;
  }
  .input-with-icon .model-icon {
    position: absolute;
    top: 50%;
    left: 9px;
    width: 15px;
    height: 15px;
    transform: translateY(-50%);
    object-fit: contain;
  }
  .model-icon {
    width: 15px;
    height: 15px;
    flex: none;
    object-fit: contain;
  }
  .refresh-models {
    display: grid;
    place-items: center;
    width: 31px;
    height: 31px;
    flex: none;
    border: 1px solid color-mix(in srgb, var(--border) 78%, transparent);
    border-radius: 7px;
    color: var(--muted);
  }
  .refresh-models:hover:not(:disabled) {
    color: var(--text);
    background: var(--bg-hover);
  }
  .refresh-models:disabled {
    opacity: 0.55;
  }
  .refresh-models span {
    display: grid;
    place-items: center;
  }
  .refresh-models span.spin {
    animation: model-spin 900ms linear infinite;
  }
  @keyframes model-spin {
    to {
      transform: rotate(360deg);
    }
  }
  .model-help,
  .model-error {
    display: block;
    margin-top: 4px;
    font-size: 8.5px;
    line-height: 1.4;
  }
  .model-help {
    color: var(--muted);
  }
  .model-error {
    color: var(--danger);
  }
  .provider-backdrop {
    position: fixed;
    inset: 0;
    z-index: 30;
    background: transparent;
  }
  .provider-menu {
    position: absolute;
    top: calc(100% + 5px);
    left: 0;
    right: 0;
    z-index: 31;
    max-height: min(300px, 55vh);
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 5px;
    border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
    border-radius: 9px;
    background: var(--bg-raised);
    box-shadow: 0 14px 34px color-mix(in srgb, #000 28%, transparent);
    animation: menu-in 130ms ease-out both;
  }
  .provider-menu button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 32px;
    padding: 0 8px;
    border-radius: 6px;
    color: var(--text);
    font-size: 10px;
    text-align: left;
  }
  .provider-menu button:hover,
  .provider-menu button:focus-visible {
    background: var(--bg-hover);
  }
  .provider-menu button.active {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--text-strong);
  }
  @keyframes menu-in {
    from {
      opacity: 0;
      transform: translateY(-3px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
