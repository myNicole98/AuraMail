<script lang="ts">
  import { Check, ChevronDown } from "lucide-svelte";
  import {
    providerById,
    providerIconUrls,
    providers,
    type AiProvider,
    type AiProviderId
  } from "../../ai/providers";
  import type { DiscoveredProvider } from "../../types";

  let {
    value,
    detected = [],
    label = "AI provider",
    onselect
  }: {
    value: AiProviderId;
    detected?: DiscoveredProvider[];
    label?: string;
    onselect: (id: AiProviderId) => void;
  } = $props();

  let open = $state(false);
  let info = $derived(providerById(value));
  let icons = $derived(providerIconUrls(value));
  let detectedIds = $derived(new Set(detected.map((item) => item.id)));
  let detectedProviders = $derived(
    providers.filter((provider) => detectedIds.has(provider.id))
  );
  let otherProviders = $derived(
    providers.filter((provider) => !detectedIds.has(provider.id))
  );

  function choose(id: AiProviderId) {
    open = false;
    onselect(id);
  }
</script>

{#snippet providerOption(provider: AiProvider)}
  {@const optionIcons = providerIconUrls(provider.id)}
  {@const selected = value === provider.id}
  {@const isDetected = detectedIds.has(provider.id)}
  <button
    type="button"
    class:active={selected}
    role="option"
    aria-selected={selected}
    onclick={() => choose(provider.id)}
  >
    <span class="provider-icon-wrap" class:has-dark={Boolean(optionIcons.dark)}>
      {#if optionIcons.light}<img
          class="provider-icon icon-light"
          src={optionIcons.light}
          alt=""
        />{/if}
      {#if optionIcons.dark}<img
          class="provider-icon icon-dark"
          src={optionIcons.dark}
          alt=""
        />{/if}
    </span>
    <span class="provider-label">{provider.label}</span>
    <span class="trailing">
      {#if isDetected}<span
          class="detected"
          title="Detected on this computer"
        ></span>{/if}
      {#if selected}<Check size={14} />{/if}
    </span>
  </button>
{/snippet}

<div class="provider-select">
  <button
    class="provider-trigger"
    class:open
    type="button"
    aria-label={label}
    aria-haspopup="listbox"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    <span class="provider-icon-wrap" class:has-dark={Boolean(icons.dark)}>
      {#if icons.light}<img class="provider-icon icon-light" src={icons.light} alt="" />{/if}
      {#if icons.dark}<img class="provider-icon icon-dark" src={icons.dark} alt="" />{/if}
    </span>
    <span class="provider-label">{info.label}</span>
    <ChevronDown size={14} />
  </button>
  {#if open}
    <button
      class="provider-backdrop"
      type="button"
      aria-label="Close provider menu"
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
      {#if detectedProviders.length > 0}
        <div class="menu-group-label">Detected locally</div>
        {#each detectedProviders as provider (provider.id)}
          {@render providerOption(provider)}
        {/each}
        <div class="menu-separator"></div>
        <div class="menu-group-label">Other providers</div>
      {/if}
      {#each otherProviders as provider (provider.id)}
        {@render providerOption(provider)}
      {/each}
    </div>
  {/if}
</div>

<style>
  .provider-select {
    position: relative;
    min-width: 0;
  }
  .provider-trigger {
    display: flex;
    align-items: center;
    gap: 8px;
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
  .provider-icon-wrap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    flex: none;
  }
  .provider-icon {
    width: 16px;
    height: 16px;
    object-fit: contain;
  }
  .provider-icon.icon-dark {
    display: none;
  }
  :global(html[data-theme-dark="true"]) .provider-icon-wrap.has-dark .provider-icon.icon-light {
    display: none;
  }
  :global(html[data-theme-dark="true"]) .provider-icon-wrap.has-dark .provider-icon.icon-dark {
    display: block;
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
  .trailing {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    margin-left: auto;
    color: var(--accent);
  }
  .detected {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--success);
  }
  .menu-group-label {
    padding: 5px 8px 3px;
    color: var(--muted);
    font-size: 8px;
    font-weight: 650;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }
  .menu-separator {
    height: 1px;
    margin: 4px 5px;
    background: color-mix(in srgb, var(--border) 60%, transparent);
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
