<script lang="ts">
  let { avatar = true }: { avatar?: boolean } = $props();
</script>

<div class="mock-row">
  <span class="mock-bar"></span>
  <span class="mock-dot"></span>
  {#if avatar}
    <span class="mock-avatar">AL</span>
  {/if}
  <span class="mock-main">
    <span class="mock-top">
      <span class="mock-sender">Ada Lovelace</span>
      <span class="mock-time">10:24</span>
    </span>
    <span class="mock-subject">Project update</span>
    <span class="mock-snippet">
      Here is the latest project update and what we need to do next.
    </span>
  </span>
</div>

<style>
  .mock-row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 9px 8px 9px 6px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 38%, transparent);
    background: var(--bg);
    text-align: left;
  }
  .mock-bar {
    position: absolute;
    top: 8px;
    bottom: 8px;
    left: 3px;
    width: 3px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 58%, transparent);
  }
  .mock-dot {
    width: 6px;
    height: 6px;
    margin-top: 7px;
    flex: none;
    border-radius: 50%;
    background: color-mix(in srgb, var(--accent) 0%, transparent);
  }
  .mock-avatar {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    flex: none;
    margin-right: 6px;
    overflow: hidden;
    border-radius: 50%;
    background: color-mix(in srgb, var(--accent) 22%, var(--bg-hover));
    color: var(--text-strong);
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 0.03em;
  }
  .mock-main {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .mock-top {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .mock-sender {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--text);
    font-size: 12.5px;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mock-time {
    display: inline-flex;
    align-items: center;
    flex: none;
    color: var(--muted);
    font-size: 10.5px;
  }
  .mock-subject {
    overflow: hidden;
    color: var(--text);
    font-size: 12.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mock-snippet {
    display: -webkit-box;
    overflow: hidden;
    color: var(--muted);
    font-size: 11.5px;
    line-height: 1.35;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }
</style>
