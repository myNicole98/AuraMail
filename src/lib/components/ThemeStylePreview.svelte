<script lang="ts">
  import type { ThemeBackground, ThemePanes } from "../settings";

  let {
    background,
    panes,
    compact = false
  }: {
    background: ThemeBackground;
    panes: ThemePanes;
    compact?: boolean;
  } = $props();
</script>

<span
  class="style-preview"
  class:compact
  class:glass={background === "glass"}
  class:rounded={panes === "rounded"}
  aria-hidden="true"
>
  <span class="style-preview-glow one"></span>
  <span class="style-preview-glow two"></span>
  <span class="style-preview-side">
    <span class="style-preview-brand"></span>
    <span class="style-preview-nav active"></span>
    <span class="style-preview-nav"></span>
    <span class="style-preview-nav short"></span>
  </span>
  <span class="style-preview-main">
    <span class="style-preview-top">
      <span class="style-preview-search"></span>
      <span class="style-preview-avatar"></span>
    </span>
    <span class="style-preview-rows">
      <span class="style-preview-row active">
        <span class="style-preview-dot"></span>
        <span class="style-preview-lines"><span></span><span></span></span>
      </span>
      <span class="style-preview-row">
        <span class="style-preview-dot"></span>
        <span class="style-preview-lines"><span></span><span></span></span>
      </span>
      <span class="style-preview-row">
        <span class="style-preview-dot"></span>
        <span class="style-preview-lines"><span></span><span></span></span>
      </span>
    </span>
  </span>
</span>

<style>
  .style-preview {
    position: relative;
    display: flex;
    height: 82px;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--border) 45%, transparent);
    border-radius: 6px;
    background: var(--bg);
  }
  .style-preview.compact {
    height: 54px;
  }
  .style-preview-glow {
    position: absolute;
    border-radius: 50%;
    filter: blur(18px);
    opacity: 0;
    pointer-events: none;
  }
  .style-preview-glow.one {
    top: -18%;
    left: 28%;
    width: 72%;
    height: 92%;
    background: var(--accent);
  }
  .style-preview-glow.two {
    bottom: -18%;
    left: -12%;
    width: 58%;
    height: 82%;
    background: var(--info);
  }
  .style-preview.glass .style-preview-glow.one {
    opacity: 0.62;
  }
  .style-preview.glass .style-preview-glow.two {
    opacity: 0.46;
  }
  .style-preview-side {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 27%;
    flex: none;
    padding: 6px 5px;
    border-right: 1px solid color-mix(in srgb, var(--border) 45%, transparent);
    background: var(--bg-raised);
  }
  .style-preview-main {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    background: var(--bg);
  }
  .style-preview.glass .style-preview-side {
    background: color-mix(in srgb, var(--bg-raised) 76%, transparent);
  }
  .style-preview.glass .style-preview-main {
    background: color-mix(in srgb, var(--bg) 74%, transparent);
  }
  .style-preview.rounded {
    padding: 3px;
    gap: 3px;
    background: transparent;
  }
  .style-preview.rounded .style-preview-side,
  .style-preview.rounded .style-preview-main {
    border: 1px solid color-mix(in srgb, var(--text) 12%, transparent);
    border-radius: 4px;
  }
  .style-preview.rounded .style-preview-side {
    border-right: 1px solid color-mix(in srgb, var(--text) 12%, transparent);
  }
  .style-preview.rounded:not(.glass) .style-preview-side,
  .style-preview.rounded:not(.glass) .style-preview-main {
    background: var(--bg-raised);
  }
  .style-preview-brand {
    width: 56%;
    height: 4px;
    margin-bottom: 2px;
    border-radius: 2px;
    background: var(--accent);
    opacity: 0.8;
  }
  .style-preview-nav {
    width: 100%;
    height: 4px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--muted) 28%, transparent);
  }
  .style-preview-nav.active {
    background: color-mix(in srgb, var(--accent) 52%, transparent);
  }
  .style-preview-nav.short {
    width: 72%;
  }
  .style-preview-top {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 7px 4px;
  }
  .style-preview-search {
    flex: 1;
    height: 8px;
    border-radius: 4px;
    border: 1px solid color-mix(in srgb, var(--border) 45%, transparent);
    background: color-mix(in srgb, var(--bg-hover) 72%, transparent);
  }
  .style-preview-avatar {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
  }
  .style-preview-rows {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 0 6px;
  }
  .style-preview-row {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 4px 3px;
    border-radius: 3px;
  }
  .style-preview-row.active {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .style-preview-dot {
    width: 3px;
    height: 3px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0.8;
  }
  .style-preview-lines {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .style-preview-lines span {
    width: 100%;
    height: 2px;
    border-radius: 1px;
    background: color-mix(in srgb, var(--muted) 36%, transparent);
  }
  .style-preview-lines span:first-child {
    width: 78%;
  }
</style>
