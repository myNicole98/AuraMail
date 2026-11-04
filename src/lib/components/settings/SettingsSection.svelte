<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    description = "",
    action = undefined,
    children
  }: {
    title: string;
    description?: string;
    action?: Snippet;
    children: Snippet;
  } = $props();
</script>

<section class="settings-section">
  <div class="section-head">
    <h3>{title}</h3>
    {#if description}<p>{description}</p>{/if}
    {#if action}{@render action()}{/if}
  </div>
  <div class="section-body">{@render children()}</div>
</section>

<style>
  .settings-section {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    gap: 32px;
    padding: 26px 0;
    border-top: 1px solid var(--border);
  }

  .settings-section:first-child {
    padding-top: 0;
    border-top: 0;
  }

  .section-head {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 7px;
  }

  .section-head h3 {
    margin: 0;
    color: var(--text-strong);
    font-size: 20px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .section-head p {
    margin: 0;
    color: var(--muted);
    font-size: 12px;
    line-height: 1.45;
  }

  .section-body {
    display: flex;
    min-width: 0;
    flex-direction: column;
  }

  @media (max-width: 760px) {
    .settings-section {
      grid-template-columns: minmax(0, 1fr);
      gap: 8px;
      padding: 20px 0;
    }
  }
</style>
