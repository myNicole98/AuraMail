<script lang="ts">
  import { onMount } from "svelte";
  import {
    MAX_UI_SCALE,
    MIN_UI_SCALE,
    normalizeUiScale,
    UI_SCALE_LABELS,
    snapUiScale
  } from "../../settings";
  import { appearance } from "../../stores/appearance.svelte";
  import ToggleSwitch from "../ToggleSwitch.svelte";
  import ThemedSelect from "../ThemedSelect.svelte";
  import { COLOR_VISION_MODES, type ColorVisionMode } from "../../theme/colorVision";

  const uiScaleMinorStops = [75, 125, 175, 225, 275];

  let uiScalePreview = $state(appearance.uiScale);
  let uiScaleText = $state(String(appearance.uiScale));

  function syncUiScale(percent: number) {
    uiScalePreview = percent;
    uiScaleText = String(percent);
  }

  function setUiScale(percent: number) {
    const next = snapUiScale(percent);
    syncUiScale(next);
    appearance.setUiScale(next);
  }

  function previewUiScale(event: Event) {
    const next = snapUiScale(Number((event.currentTarget as HTMLInputElement).value));
    syncUiScale(next);
  }

  function commitUiScale() {
    if (uiScalePreview !== appearance.uiScale) setUiScale(uiScalePreview);
  }

  function commitUiScaleInput() {
    const raw = uiScaleText.trim();
    const numeric = Number(raw);
    const next =
      raw !== "" && Number.isFinite(numeric)
        ? normalizeUiScale(numeric)
        : appearance.uiScale;
    syncUiScale(next);
    if (next !== appearance.uiScale) appearance.setUiScale(next);
  }

  function commitUiScaleOnEnter(event: KeyboardEvent) {
    if (event.key !== "Enter") return;
    commitUiScaleInput();
    (event.currentTarget as HTMLInputElement).blur();
  }

  function scalePosition(value: number): string {
    const ratio = (value - MIN_UI_SCALE) / (MAX_UI_SCALE - MIN_UI_SCALE);
    const thumb = 22;
    return `calc(${ratio * 100}% + ${thumb / 2 - thumb * ratio}px)`;
  }

  function scaleFill(value: number): string {
    return scalePosition(value);
  }

  function setReduceMotion(value: boolean) {
    appearance.setReduceMotion(value);
  }

  function setHighContrast(value: boolean) {
    appearance.setHighContrast(value);
  }

  function setColorVision(mode: ColorVisionMode) {
    appearance.setColorVision(mode);
  }

  onMount(() => {
    syncUiScale(appearance.uiScale);
  });
</script>

<div class="settings-inner">
  <div class="settings-label">Display</div>
  <section class="settings-panel accessibility-panel">
    <div class="ui-scale-head">
      <h4>UI size</h4>
      <label class="ui-scale-value">
        <input
          type="number"
          min={MIN_UI_SCALE}
          max={MAX_UI_SCALE}
          step="1"
          value={uiScaleText}
          aria-label="UI size percentage"
          oninput={(event) => (uiScaleText = event.currentTarget.value)}
          onchange={commitUiScaleInput}
          onblur={commitUiScaleInput}
          onkeydown={commitUiScaleOnEnter}
        />
        <span>%</span>
      </label>
    </div>
    <div class="ui-scale-track">
      <div class="ui-scale-input-wrap">
        <span class="ui-scale-rail" aria-hidden="true">
          <span class="ui-scale-fill" style:width={scaleFill(uiScalePreview)}></span>
        </span>
        <input
          id="ui-scale"
          class="ui-scale-slider"
          type="range"
          min={MIN_UI_SCALE}
          max={MAX_UI_SCALE}
          step="1"
          value={uiScalePreview}
          aria-label="UI size"
          aria-valuetext={`${uiScalePreview}%`}
          oninput={previewUiScale}
          onchange={commitUiScale}
          onpointerup={commitUiScale}
          onkeyup={commitUiScale}
          onblur={commitUiScale}
        />
        {#each UI_SCALE_LABELS as stop (stop)}
          <span class="ui-scale-major" style:left={scalePosition(stop)}></span>
        {/each}
        {#each uiScaleMinorStops as stop (stop)}
          <span class="ui-scale-minor" style:left={scalePosition(stop)}></span>
        {/each}
        <span class="ui-scale-thumb" style:left={scalePosition(uiScalePreview)}></span>
      </div>
      <div class="ui-scale-stops">
        {#each UI_SCALE_LABELS as stop (stop)}
          <button
            type="button"
            class="ui-scale-stop"
            class:active={uiScalePreview === stop}
            style:left={scalePosition(stop)}
            aria-label={`Set UI size to ${stop}%`}
            onclick={() => setUiScale(stop)}
          >
            {stop}%
          </button>
        {/each}
      </div>
    </div>
  </section>

  <div class="settings-label">Comfort</div>
  <section class="settings-panel accessibility-panel">
    <div class="settings-row">
      <strong>Disable animations</strong>
      <ToggleSwitch
        checked={appearance.reduceMotion}
        label="Disable animations"
        onchange={setReduceMotion}
      />
    </div>
    <div class="settings-row">
      <strong>High contrast</strong>
      <ToggleSwitch
        checked={appearance.highContrast}
        label="High contrast"
        onchange={setHighContrast}
      />
    </div>
    <div class="settings-row">
      <strong>Color vision</strong>
      <ThemedSelect
        value={appearance.colorVision}
        options={COLOR_VISION_MODES.map((mode) => ({
          value: mode.id,
          label: mode.label
        }))}
        ariaLabel="Color vision"
        onchange={(value) => setColorVision(value as ColorVisionMode)}
      />
    </div>
  </section>
</div>

<style>
  .accessibility-panel {
    padding: 15px 16px 8px;
  }
  .ui-scale-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .ui-scale-head h4 {
    margin: 0;
    color: var(--text);
    font-size: 11.5px;
    font-weight: 620;
  }
  .ui-scale-value {
    display: flex;
    align-items: center;
    flex: none;
    gap: 4px;
    color: var(--muted);
    font-size: 10px;
  }
  .ui-scale-value input {
    width: 76px;
    min-width: 76px;
    height: 28px;
    padding: 0 6px;
    border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    border-radius: 6px;
    background: color-mix(in srgb, var(--bg) 55%, var(--bg-raised));
    color: var(--text);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
    text-align: center;
  }
  .ui-scale-value input:focus-visible {
    border-color: color-mix(in srgb, var(--accent) 55%, var(--border));
    outline: none;
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .ui-scale-track {
    margin-top: 10px;
  }
  .ui-scale-input-wrap {
    position: relative;
    height: 28px;
  }
  .ui-scale-rail {
    position: absolute;
    top: 8px;
    right: 0;
    left: 0;
    height: 12px;
    overflow: hidden;
    border-radius: 999px;
    background: color-mix(in srgb, var(--border) 65%, transparent);
  }
  .ui-scale-fill {
    display: block;
    height: 100%;
    border-radius: 999px;
    background: var(--accent);
  }
  .ui-scale-slider {
    position: absolute;
    inset: 0;
    display: block;
    width: 100%;
    height: 28px;
    margin: 0;
    padding: 0;
    border: 0 !important;
    outline: none !important;
    box-shadow: none !important;
    cursor: pointer;
    appearance: none;
    -webkit-appearance: none;
    background: transparent;
  }
  .ui-scale-slider::-webkit-slider-runnable-track {
    background: transparent;
  }
  .ui-scale-slider::-moz-range-track {
    background: transparent;
  }
  .ui-scale-slider::-webkit-slider-thumb {
    width: 22px;
    height: 22px;
    border: 0;
    opacity: 0;
    appearance: none;
    background: transparent;
    box-shadow: none;
  }
  .ui-scale-slider::-moz-range-thumb {
    width: 22px;
    height: 22px;
    border: 0;
    opacity: 0;
    background: transparent;
    box-shadow: none;
  }
  .ui-scale-slider:focus-visible ~ .ui-scale-thumb {
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--accent) 75%, transparent),
      0 0 0 4px color-mix(in srgb, var(--accent) 22%, transparent),
      0 2px 6px rgb(0 0 0 / 22%);
  }
  .ui-scale-major,
  .ui-scale-minor {
    position: absolute;
    width: 1px;
    border-radius: 1px;
    transform: translateX(-50%);
  }
  .ui-scale-major {
    top: 21px;
    height: 8px;
    background: color-mix(in srgb, var(--muted) 70%, transparent);
  }
  .ui-scale-minor {
    top: 22px;
    height: 5px;
    background: color-mix(in srgb, var(--muted) 42%, transparent);
  }
  .ui-scale-thumb {
    position: absolute;
    top: 3px;
    width: 22px;
    height: 22px;
    border: 3px solid var(--bg);
    border-radius: 50%;
    pointer-events: none;
    background: var(--accent);
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--accent) 60%, transparent),
      0 2px 6px rgb(0 0 0 / 22%);
    transform: translateX(-50%);
  }
  .ui-scale-stops {
    position: relative;
    height: 15px;
    margin: 14px 0 0;
  }
  .ui-scale-stop {
    position: absolute;
    top: 0;
    padding: 0;
    color: var(--muted);
    font-size: 9.5px;
    font-variant-numeric: tabular-nums;
    transform: translateX(-50%);
    white-space: nowrap;
  }
  .ui-scale-stop:hover,
  .ui-scale-stop.active {
    color: var(--accent);
  }
</style>
