<script lang="ts">
  import StepHeader from "../StepHeader.svelte";

  let { onNext }: { onNext: () => void } = $props();
</script>

<div class="welcome">
  <StepHeader
    kicker="Getting started"
    title="Welcome to AuraMail."
    description="A calm, privacy-first inbox."
  />
  <div class="actions">
    <button class="primary" type="button" onclick={onNext}>Get started</button>
  </div>
</div>

<style>
  .welcome { display: flex; flex-direction: column; gap: 26px; }
  .actions { display: flex; justify-content: flex-start; }
  .primary {
    min-width: 120px;
    padding: 10px 17px;
    border-radius: 9px;
    background: var(--accent);
    color: var(--bg);
    font-size: 10.5px;
    font-weight: 650;
    box-shadow: 0 8px 24px color-mix(in srgb, var(--accent) 24%, transparent);
  }
  .primary:hover { filter: brightness(1.08); }
</style>
