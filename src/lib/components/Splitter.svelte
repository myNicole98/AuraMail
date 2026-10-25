<script lang="ts">
  let {
    onPointerDown,
    onKeyboardResize,
    label = "Resize panes",
    value = 50,
    min = 0,
    max = 100,
    orientation = "vertical"
  }: {
    onPointerDown: (event: PointerEvent) => void;
    onKeyboardResize: (delta: number) => void;
    label?: string;
    value?: number;
    min?: number;
    max?: number;
    orientation?: "vertical" | "horizontal";
  } = $props();

  function onKeydown(event: KeyboardEvent) {
    const vertical = orientation === "vertical";
    const keys = vertical
      ? ["ArrowLeft", "ArrowRight"]
      : ["ArrowUp", "ArrowDown"];
    if (!keys.includes(event.key)) return;
    event.preventDefault();
    const backward = vertical ? "ArrowLeft" : "ArrowUp";
    onKeyboardResize(event.key === backward ? -16 : 16);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="splitter"
  class:horizontal={orientation === "horizontal"}
  role="separator"
  aria-orientation={orientation}
  aria-label={label}
  aria-valuenow={value}
  aria-valuemin={min}
  aria-valuemax={max}
  tabindex={0}
  onpointerdown={onPointerDown}
  onkeydown={onKeydown}
></div>

<style>
  .splitter {
    width: 1px;
    height: 100%;
    padding: 0;
    border: 0;
    cursor: col-resize;
    background: color-mix(in srgb, var(--border) 28%, var(--bg));
    position: relative;
    z-index: 2;
    touch-action: none;
  }
  .splitter::after {
    content: "";
    position: absolute;
    inset: 0 -8px;
  }
  .splitter.horizontal {
    width: 100%;
    height: 1px;
    cursor: row-resize;
  }
  .splitter.horizontal::after {
    inset: -8px 0;
  }
  /* The resize cursor is enough feedback. Do not light up the 1px line while
     the pointer merely passes near it; keyboard focus remains visible. */
  .splitter:focus-visible {
    background: var(--accent);
    outline: none;
  }
</style>
