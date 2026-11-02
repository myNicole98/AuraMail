<script lang="ts">
  let {
    checked,
    label,
    onchange,
    disabled = false,
    small = false
  }: {
    checked: boolean;
    label: string;
    onchange: (value: boolean) => void;
    disabled?: boolean;
    small?: boolean;
  } = $props();
</script>

<button
  class="switch"
  class:small
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span></span>
</button>

<style>
  .switch {
    position: relative;
    width: 38px;
    height: 22px;
    flex: none;
    padding: 0;
    border: 1px solid color-mix(in srgb, var(--border) 85%, transparent);
    border-radius: 99px;
    background: color-mix(in srgb, var(--muted) 22%, var(--bg));
    transition: background 160ms ease, border-color 160ms ease;
  }
  .switch.small {
    width: 32px;
    height: 18px;
  }
  .switch span {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--text-strong);
    box-shadow: 0 1px 4px color-mix(in srgb, var(--bg) 50%, transparent);
    transition: transform 180ms cubic-bezier(0.22, 0.8, 0.2, 1);
  }
  .switch[aria-checked="true"] {
    border-color: var(--accent);
    background: var(--accent);
  }
  .switch.small span {
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
  }
  .switch[aria-checked="true"] span {
    transform: translateX(16px);
    background: var(--bg);
  }
  .switch.small[aria-checked="true"] span {
    transform: translateX(14px);
  }
  .switch:disabled { opacity: 0.5; cursor: default; }
  .switch:focus-visible { border-color: var(--text-strong); }
</style>
