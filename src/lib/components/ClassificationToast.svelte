<script lang="ts">
  import { onMount } from "svelte";
  import { LoaderCircle, Tags, X } from "lucide-svelte";
  import { appearance } from "../stores/appearance.svelte";
  import type { ClassificationProgress } from "../types";

  let {
    message,
    progress = null,
    action = null,
    onAbort,
    onReclassify,
    onDismiss
  }: {
    message: string;
    progress?: ClassificationProgress | null;
    action?: "reclassify" | null;
    onAbort: () => void;
    onReclassify: () => void;
    onDismiss: () => void;
  } = $props();

  let shell = $state<HTMLDivElement | null>(null);
  let content = $state<HTMLDivElement | null>(null);
  let expanded = $state(false);
  let targetWidth = $state(36);

  onMount(() => {
    const reduced =
      appearance.reduceMotion ||
      (typeof window.matchMedia === "function" &&
        window.matchMedia("(prefers-reduced-motion: reduce)").matches);
    if (reduced) {
      expanded = true;
      return;
    }
    const timer = setTimeout(() => (expanded = true), 260);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    // Track the rendered copy so the shell can grow or shrink with it.
    message;
    progress?.done;
    progress?.total;
    progress?.aborting;
    action;
    if (!content || !shell) return;
    const available = Math.max(
      120,
      (shell.parentElement?.clientWidth ?? 640) - 24
    );
    targetWidth = Math.min(36 + content.offsetWidth, available);
  });
</script>

<div
  bind:this={shell}
  class="toast"
  class:expanded
  style:width={expanded ? `${targetWidth}px` : "36px"}
  role="status"
  aria-live="polite"
>
  <div class="toast-content" bind:this={content}>
    <span class="toast-message">{message}</span>
    {#if action === "reclassify" && !progress}
      <button type="button" class="toast-action" onclick={onReclassify}>
        Reclassify
      </button>
    {/if}
    {#if progress?.abortable}
      <button
        type="button"
        class="toast-abort"
        disabled={progress.aborting}
        onclick={onAbort}
      >
        {progress.aborting ? "Aborting…" : "Abort"}
      </button>
    {/if}
    {#if !progress}
      <button
        type="button"
        class="toast-close"
        aria-label="Dismiss classification notice"
        onclick={onDismiss}
      >
        <X size={13} />
      </button>
    {/if}
  </div>
  <span class="toast-icon" aria-hidden="true">
    {#if progress}
      <span class="toast-spinner"><LoaderCircle size={14} /></span>
    {:else}
      <Tags size={14} />
    {/if}
  </span>
</div>

<style>
  .toast {
    position: absolute;
    right: 12px;
    bottom: 14px;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    height: 36px;
    max-width: calc(100% - 24px);
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    border-radius: 999px;
    background: var(--bg-raised);
    box-shadow: 0 8px 24px color-mix(in srgb, #000 18%, transparent);
    color: var(--text);
    font-size: 11.5px;
    line-height: 1.4;
    transition: width 360ms cubic-bezier(0.22, 0.8, 0.2, 1);
  }
  .toast-content {
    display: flex;
    align-items: center;
    gap: 9px;
    flex: none;
    padding: 0 10px 0 12px;
    opacity: 0;
    transform: translateX(-5px);
    transition:
      opacity 220ms 140ms ease-out,
      transform 220ms 140ms ease-out;
  }
  .toast.expanded .toast-content {
    opacity: 1;
    transform: translateX(0);
  }
  .toast-icon {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    flex: none;
    color: var(--accent);
    animation: toast-icon-in 360ms cubic-bezier(0.22, 0.8, 0.2, 1) both;
  }
  .toast-message {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .toast :global(svg) {
    display: block;
    flex: none;
    color: var(--accent);
  }
  .toast-spinner {
    display: grid;
    place-items: center;
    width: 14px;
    height: 14px;
    line-height: 0;
  }
  .toast button {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    flex: none;
    border-radius: 5px;
    color: var(--muted);
  }
  .toast button:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .toast .toast-action,
  .toast .toast-abort {
    display: inline-flex;
    width: auto;
    padding: 0 8px;
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--accent);
    font-size: 10.5px;
    font-weight: 650;
  }
  .toast .toast-abort:disabled {
    cursor: default;
    opacity: 0.65;
  }
  .toast .toast-close {
    color: var(--muted);
  }
  .toast-spinner :global(svg) {
    animation: spin 900ms linear infinite;
  }
  @keyframes toast-icon-in {
    0% {
      opacity: 0;
      transform: translateY(9px);
    }
    70% {
      opacity: 1;
      transform: translateY(-1px);
    }
    100% {
      opacity: 1;
      transform: translateY(0);
    }
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
