<script lang="ts">
  let {
    checked,
    indeterminate = false,
    label,
    ariaLabel,
    disabled = false,
    onchange
  }: {
    checked: boolean;
    indeterminate?: boolean;
    label?: string;
    ariaLabel?: string;
    disabled?: boolean;
    onchange: (checked: boolean) => void;
  } = $props();

  let input = $state<HTMLInputElement | null>(null);

  $effect(() => {
    if (input) input.indeterminate = indeterminate;
  });
</script>

<label
  class="checkbox"
  class:checked
  class:indeterminate
  class:disabled
>
  <input
    bind:this={input}
    type="checkbox"
    {checked}
    {disabled}
    aria-label={ariaLabel ?? label}
    onchange={(event) => onchange(event.currentTarget.checked)}
  />
  <span class="box" aria-hidden="true">
    <svg class="check" viewBox="0 0 12 12">
      <path d="M2.6 6.2 4.9 8.5 9.4 3.7" />
    </svg>
    <span class="dash"></span>
  </span>
  {#if label}<span class="label">{label}</span>{/if}
</label>

<style>
  .checkbox {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    flex: none;
    color: var(--text);
    font-size: 11px;
    font-weight: 600;
    line-height: 1.2;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }
  .checkbox input {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: 0;
    opacity: 0;
    pointer-events: none;
  }
  .box {
    position: relative;
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    flex: none;
    border: 2px solid color-mix(in srgb, var(--muted) 54%, transparent);
    border-radius: 6px;
    background: color-mix(in srgb, var(--bg) 68%, transparent);
    transition:
      border-color 150ms ease,
      background 150ms ease,
      box-shadow 150ms ease,
      transform 120ms ease;
  }
  .checkbox:hover .box {
    border-color: color-mix(in srgb, var(--accent) 58%, var(--border));
    background: color-mix(in srgb, var(--accent) 7%, var(--bg));
  }
  .checkbox:active .box {
    transform: scale(0.88);
  }
  .checkbox.checked .box,
  .checkbox.indeterminate .box {
    border-color: var(--accent);
    background: linear-gradient(
      135deg,
      color-mix(in srgb, var(--accent) 86%, #fff),
      var(--accent)
    );
    box-shadow:
      0 0 0 3px color-mix(in srgb, var(--accent) 13%, transparent),
      inset 0 1px 0 color-mix(in srgb, #fff 24%, transparent);
  }
  .checkbox.checked .box {
    animation: checkbox-pop 190ms cubic-bezier(0.22, 0.8, 0.2, 1);
  }
  .check {
    width: 12px;
    height: 12px;
    overflow: visible;
    fill: none;
    stroke: var(--bg);
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-dasharray: 14;
    stroke-dashoffset: 14;
    transition: stroke-dashoffset 180ms cubic-bezier(0.22, 0.8, 0.2, 1) 30ms;
  }
  .checkbox.checked .check {
    stroke-dashoffset: 0;
  }
  .checkbox.indeterminate .check {
    stroke-dashoffset: 14;
  }
  .dash {
    position: absolute;
    width: 8px;
    height: 2px;
    border-radius: 2px;
    background: var(--bg);
    opacity: 0;
    transform: scaleX(0.25);
    transition:
      opacity 120ms ease,
      transform 160ms cubic-bezier(0.22, 0.8, 0.2, 1);
  }
  .checkbox.indeterminate .dash {
    opacity: 1;
    transform: scaleX(1);
  }
  .checkbox input:focus-visible + .box {
    border-color: var(--accent);
  }
  .checkbox.disabled {
    cursor: default;
    opacity: 0.42;
  }
  .checkbox.disabled .box {
    transform: none;
  }
  .label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @keyframes checkbox-pop {
    0% {
      transform: scale(0.82);
    }
    62% {
      transform: scale(1.07);
    }
    100% {
      transform: scale(1);
    }
  }
</style>
