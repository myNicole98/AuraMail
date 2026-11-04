<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    description = "",
    indent = false,
    children
  }: {
    label: string;
    description?: string;
    indent?: boolean;
    children: Snippet;
  } = $props();
</script>

<div class="settings-row" class:indent>
  <div class="row-text">
    <strong>{label}</strong>
    {#if description}<span>{description}</span>{/if}
  </div>
  <div class="row-control">{@render children()}</div>
</div>

<style>
  .settings-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    min-height: 40px;
    padding: 8px 0;
  }

  .settings-row.indent {
    margin-left: 10px;
    padding-left: 14px;
    border-left: 2px solid color-mix(in srgb, var(--accent) 35%, var(--border));
  }

  .row-text {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
  }

  .row-text strong {
    color: var(--text);
    font-size: 12.5px;
    font-weight: 500;
  }

  .row-text span {
    color: var(--muted);
    font-size: 11.5px;
    line-height: 1.4;
  }

  .row-control {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    flex: none;
    min-width: 0;
  }
</style>
