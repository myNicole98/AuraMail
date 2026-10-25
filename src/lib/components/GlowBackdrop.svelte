<script lang="ts">
  import { onMount } from "svelte";
  import { applyBayerDither } from "../theme/dither";

  type Rgba = { r: number; g: number; b: number; a: number };
  type Glow = Rgba & { x: number; y: number; rx: number; ry: number };

  let {
    force = false,
    exportImage = false
  }: { force?: boolean; exportImage?: boolean } = $props();

  const DITHER_AMPLITUDE = 4;
  const GLOW_SHAPES = [
    { token: "--glow-1", x: 0.16, y: 0.14, rx: 0.58, ry: 0.54 },
    { token: "--glow-2", x: 0.78, y: 0.1, rx: 0.52, ry: 0.48 },
    { token: "--glow-3", x: 0.88, y: 0.78, rx: 0.62, ry: 0.56 },
    { token: "--glow-4", x: 0.22, y: 0.88, rx: 0.5, ry: 0.58 },
    { token: "--glow-5", x: 0.58, y: 0.56, rx: 0.44, ry: 0.42 }
  ];
  const FORCED_GLOWS_DARK = [
    "color-mix(in srgb, var(--accent) 30%, transparent)",
    "color-mix(in srgb, var(--info) 28%, transparent)",
    "color-mix(in srgb, var(--accent-alt) 30%, transparent)",
    "color-mix(in srgb, var(--accent) 18%, transparent)",
    "color-mix(in srgb, var(--info) 10%, transparent)"
  ];
  const FORCED_GLOWS_LIGHT = [
    "color-mix(in srgb, var(--accent) 9%, transparent)",
    "color-mix(in srgb, var(--info) 9%, transparent)",
    "color-mix(in srgb, var(--accent-alt) 10%, transparent)",
    "color-mix(in srgb, var(--accent) 6%, transparent)",
    "color-mix(in srgb, var(--info) 5%, transparent)"
  ];

  let canvas: HTMLCanvasElement;
  let frame = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function parseColor(value: string): Rgba | null {
    const srgb = value.match(/color\(\s*srgb\s+([^)]+)\)/i);
    if (srgb) {
      const parts = srgb[1]
        .trim()
        .split(/[\s/]+/)
        .map((part) => Number(part));
      if (parts.length >= 3 && parts.every((part) => Number.isFinite(part))) {
        return {
          r: parts[0] * 255,
          g: parts[1] * 255,
          b: parts[2] * 255,
          a: parts[3] ?? 1
        };
      }
    }

    const match = value.match(/rgba?\(([^)]+)\)/i);
    if (!match) return null;
    const parts = match[1]
      .split(/[,\s/]+/)
      .map((part) => Number(part))
      .filter((part) => Number.isFinite(part));
    if (parts.length < 3) return null;
    return {
      r: parts[0],
      g: parts[1],
      b: parts[2],
      a: parts[3] ?? 1
    };
  }

  function draw() {
    if (!canvas) return;
    const root = document.documentElement;
    if (
      (!force && root.dataset.themeGlass !== "true") ||
      root.dataset.highContrast === "true"
    ) {
      canvas.style.display = "none";
      if (exportImage) root.style.removeProperty("--glass-backdrop-image");
      return;
    }
    canvas.style.display = "block";
    if (!exportImage) root.style.removeProperty("--glass-backdrop-image");

    const probe = document.createElement("span");
    probe.style.cssText =
      "position:fixed;left:-10000px;top:-10000px;visibility:hidden;pointer-events:none";
    document.body.appendChild(probe);
    const colorValue = (css: string, fallback: Rgba): Rgba => {
      probe.style.color = css;
      const color = parseColor(getComputedStyle(probe).color);
      return color ?? fallback;
    };
    const transparent: Rgba = { r: 0, g: 0, b: 0, a: 0 };
    const base = colorValue("var(--bg)", { r: 0, g: 0, b: 0, a: 1 });
    const hasGlassTokens =
      getComputedStyle(root).getPropertyValue("--glow-1").trim() !== "";
    const useGlassTokens = !force || hasGlassTokens;
    const forcedGlows =
      root.dataset.themeDark === "false"
        ? FORCED_GLOWS_LIGHT
        : FORCED_GLOWS_DARK;
    const glows: Glow[] = GLOW_SHAPES.map((shape, index) => ({
      ...shape,
      ...colorValue(
        useGlassTokens ? `var(${shape.token})` : forcedGlows[index],
        transparent
      )
    }));
    probe.remove();

    // Render at physical resolution so the dither is not resampled by WebKit;
    // CSS-resolution scaling introduced visible circular moiré contours.
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const width = Math.max(1, Math.round(window.innerWidth * dpr));
    const height = Math.max(1, Math.round(window.innerHeight * dpr));
    canvas.width = width;
    canvas.height = height;

    const context = canvas.getContext("2d");
    if (!context) return;
    const image = context.createImageData(width, height);
    const pixels = image.data;
    const shapes = glows.map((glow) => ({
      ...glow,
      cx: glow.x * width,
      cy: glow.y * height,
      rx: glow.rx * width,
      ry: glow.ry * height
    }));

    // Composite in float precision, then dither at final 8-bit brightness.
    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        let red = base.r;
        let green = base.g;
        let blue = base.b;

        // CSS paints the first gradient on top, so composite from the last one
        // upward onto the base colour.
        for (let index = shapes.length - 1; index >= 0; index -= 1) {
          const glow = shapes[index];
          const dx = (x - glow.cx) / glow.rx;
          const dy = (y - glow.cy) / glow.ry;
          const distanceSq = dx * dx + dy * dy;
          if (distanceSq >= 0.49) continue;
          const alpha = glow.a * (1 - Math.sqrt(distanceSq) / 0.7);
          if (alpha <= 0) continue;
          red = red * (1 - alpha) + glow.r * alpha;
          green = green * (1 - alpha) + glow.g * alpha;
          blue = blue * (1 - alpha) + glow.b * alpha;
        }

        const offset = (y * width + x) * 4;
        pixels[offset] = applyBayerDither(red, x, y, DITHER_AMPLITUDE);
        pixels[offset + 1] = applyBayerDither(green, x, y, DITHER_AMPLITUDE);
        pixels[offset + 2] = applyBayerDither(blue, x, y, DITHER_AMPLITUDE);
        pixels[offset + 3] = 255;
      }
    }

    context.putImageData(image, 0, 0);
    if (exportImage) {
      root.style.setProperty(
        "--glass-backdrop-image",
        `url("${canvas.toDataURL("image/png")}")`
      );
    }
  }

  function schedule(delay = 0) {
    if (timer) clearTimeout(timer);
    if (frame) cancelAnimationFrame(frame);
    timer = setTimeout(() => {
      timer = undefined;
      frame = requestAnimationFrame(() => {
        frame = 0;
        draw();
      });
    }, delay);
  }

  function onResize() {
    schedule(120);
  }

  function onThemeApplied() {
    schedule();
  }

  $effect(() => {
    force;
    exportImage;
    if (canvas) schedule();
  });

  onMount(() => {
    schedule();
    const observer = new MutationObserver(() => schedule());
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: [
        "data-theme-background",
        "data-theme-glass",
        "data-theme-scheme",
        "data-theme-dark",
        "data-high-contrast",
        "style"
      ]
    });
    window.addEventListener("resize", onResize);
    window.addEventListener("auramail:theme-applied", onThemeApplied);
    window.addEventListener("auramail:redraw-backdrop", onThemeApplied);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", onResize);
      window.removeEventListener("auramail:theme-applied", onThemeApplied);
      window.removeEventListener("auramail:redraw-backdrop", onThemeApplied);
      if (timer) clearTimeout(timer);
      if (frame) cancelAnimationFrame(frame);
    };
  });
</script>

<canvas
  class="glow-backdrop"
  class:forced={force}
  bind:this={canvas}
  aria-hidden="true"
></canvas>

<style>
  .glow-backdrop {
    position: fixed;
    inset: 0;
    z-index: 0;
    display: block;
    width: 100vw;
    height: 100vh;
    pointer-events: none;
  }

  :global(html:not([data-theme-glass="true"]) .glow-backdrop:not(.forced)),
  :global(html[data-high-contrast="true"] .glow-backdrop) {
    display: none !important;
  }
</style>
