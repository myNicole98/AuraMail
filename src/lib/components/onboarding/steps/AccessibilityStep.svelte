<script lang="ts">
  import { Eye, Move, ScanEye, Type } from "lucide-svelte";
  import {
    MAX_UI_SCALE,
    MIN_UI_SCALE,
    snapUiScale,
    UI_SCALE_LABELS,
    UI_SCALE_STOPS
  } from "../../../settings";
  import { appearance } from "../../../stores/appearance.svelte";
  import {
    COLOR_VISION_MODES,
    type ColorVisionMode
  } from "../../../theme/colorVision";
  import ToggleSwitch from "../../ToggleSwitch.svelte";
  import StepHeader from "../StepHeader.svelte";

  let {
    onBack,
    onNext
  }: { onBack: () => void; onNext: () => void } = $props();

  let scalePreview = $state(appearance.uiScale);

  function previewScale(event: Event) {
    scalePreview = snapUiScale(Number((event.currentTarget as HTMLInputElement).value));
  }

  function commitScale() {
    if (scalePreview !== appearance.uiScale) appearance.setUiScale(scalePreview);
  }

  function scalePosition(value: number): string {
    return `${((value - MIN_UI_SCALE) / (MAX_UI_SCALE - MIN_UI_SCALE)) * 100}%`;
  }

  const fill = $derived(scalePosition(scalePreview));
  const majorStops = new Set<number>(UI_SCALE_LABELS);
</script>

<div class="accessibility">
  <StepHeader
    kicker="Accessibility"
    title="Make it comfortable."
    description="Tune reading comfort. Change it later in Settings."
  />
  <section class="control-card">
    <div class="control-head">
      <span class="icon"><Type size={14} /></span>
      <div><strong>Text size</strong></div>
      <output>{scalePreview}%</output>
    </div>
    <div class="range-wrap" style={`--fill:${fill}`}>
      <input
        type="range"
        min={MIN_UI_SCALE}
        max={MAX_UI_SCALE}
        step="1"
        value={scalePreview}
        aria-label="Text size"
        aria-valuetext={`${scalePreview}%`}
        oninput={previewScale}
        onchange={commitScale}
        onpointerup={commitScale}
        onkeyup={commitScale}
        onblur={commitScale}
      />
      <span class="ticks" aria-hidden="true">
        {#each UI_SCALE_STOPS as stop (stop)}
          <i class:major={majorStops.has(stop)} style:left={scalePosition(stop)}></i>
        {/each}
      </span>
    </div>
    <div class="range-labels">
      {#each UI_SCALE_LABELS as stop (stop)}<span>{stop}%</span>{/each}
    </div>
  </section>
  <div class="toggle-stack">
    <div class="toggle-row">
      <span class="icon"><Move size={14} /></span>
      <span class="copy"><strong>Disable animations</strong></span>
      <ToggleSwitch
        checked={appearance.reduceMotion}
        label="Disable animations"
        onchange={(value) => appearance.setReduceMotion(value)}
      />
    </div>
    <div class="toggle-row">
      <span class="icon"><ScanEye size={14} /></span>
      <span class="copy"><strong>High contrast</strong></span>
      <ToggleSwitch
        checked={appearance.highContrast}
        label="High contrast"
        onchange={(value) => appearance.setHighContrast(value)}
      />
    </div>
    <div class="toggle-row">
      <span class="icon"><Eye size={14} /></span>
      <span class="copy"><strong>Color vision</strong></span>
      <select
        class="color-vision"
        aria-label="Color vision"
        value={appearance.colorVision}
        onchange={(event) =>
          appearance.setColorVision(event.currentTarget.value as ColorVisionMode)}
      >
        {#each COLOR_VISION_MODES as mode (mode.id)}
          <option value={mode.id}>{mode.label}</option>
        {/each}
      </select>
    </div>
  </div>
  <div class="actions">
    <button class="ghost" type="button" onclick={onBack}>Back</button>
    <button class="primary" type="button" onclick={onNext}>Continue</button>
  </div>
</div>

<style>
  .accessibility { display: flex; flex-direction: column; gap: 14px; }
  .control-card,
  .toggle-row {
    border: 1px solid color-mix(in srgb, var(--border) 72%, transparent);
    border-radius: 12px;
    background: color-mix(in srgb, var(--bg-raised) 72%, var(--bg));
  }
  .control-card { padding: 13px 14px; }
  .control-head,
  .toggle-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .control-head strong,
  .copy strong { display: block; color: var(--text-strong); font-size: 11px; }
  .control-head output { margin-left: auto; color: var(--accent); font-size: 11px; font-weight: 700; }
  .icon {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex: none;
    border-radius: 8px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    color: var(--accent);
  }
  .range-wrap { position: relative; margin-top: 15px; }
  input[type="range"] {
    width: 100%;
    height: 4px;
    border-radius: 99px;
    appearance: none;
    background: linear-gradient(to right, var(--accent) var(--fill), color-mix(in srgb, var(--border) 75%, transparent) var(--fill));
  }
  input[type="range"]::-webkit-slider-thumb {
    width: 16px;
    height: 16px;
    appearance: none;
    border: 3px solid var(--bg-raised);
    border-radius: 50%;
    background: var(--text-strong);
    box-shadow: 0 1px 6px color-mix(in srgb, var(--bg) 55%, transparent);
  }
  .ticks {
    position: absolute;
    inset: -4px 0 auto;
    height: 12px;
    pointer-events: none;
  }
  .ticks i {
    position: absolute;
    top: 6px;
    width: 1px;
    height: 4px;
    border-radius: 99px;
    background: color-mix(in srgb, var(--muted) 38%, transparent);
    transform: translateX(-50%);
  }
  .ticks i.major {
    height: 6px;
    background: color-mix(in srgb, var(--muted) 58%, transparent);
  }
  .range-labels { display: flex; justify-content: space-between; margin-top: 7px; color: var(--muted); font-size: 8px; }
  .toggle-stack { display: flex; flex-direction: column; gap: 8px; }
  .toggle-row { padding: 11px 13px; cursor: default; }
  .copy { flex: 1; min-width: 0; }
  .color-vision {
    min-width: 0;
    max-width: 122px;
    height: 27px;
    flex: none;
    padding: 0 22px 0 8px;
    border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    border-radius: 7px;
    appearance: none;
    -webkit-appearance: none;
    background-color: color-mix(in srgb, var(--bg) 55%, var(--bg-raised));
    background-image:
      linear-gradient(45deg, transparent 50%, var(--muted) 50%),
      linear-gradient(135deg, var(--muted) 50%, transparent 50%);
    background-position:
      calc(100% - 12px) 50%,
      calc(100% - 8px) 50%;
    background-repeat: no-repeat;
    background-size: 4px 4px, 4px 4px;
    color: var(--text);
    font-size: 9.5px;
    text-overflow: ellipsis;
  }
  .color-vision option { background: var(--bg-raised); color: var(--text); }
  .actions { display: flex; justify-content: space-between; margin-top: 2px; }
  .ghost, .primary { padding: 9px 16px; border-radius: 8px; font-size: 10.5px; font-weight: 650; }
  .ghost { border: 1px solid color-mix(in srgb, var(--border) 80%, transparent); color: var(--muted); }
  .primary { background: var(--accent); color: var(--bg); }
  .primary:hover { filter: brightness(1.08); }
</style>
