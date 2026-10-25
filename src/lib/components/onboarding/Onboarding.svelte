<script lang="ts">
  import { tick } from "svelte";
  import { markOnboardingComplete, ONBOARDING_STEPS } from "../../onboarding";
  import type { Account } from "../../types";
  import OnboardingPreview from "./OnboardingPreview.svelte";
  import WelcomeStep from "./steps/WelcomeStep.svelte";
  import AccessibilityStep from "./steps/AccessibilityStep.svelte";
  import AppearanceStep from "./steps/AppearanceStep.svelte";
  import AiStep from "./steps/AiStep.svelte";
  import AccountStep from "./steps/AccountStep.svelte";
  import ReadyStep from "./steps/ReadyStep.svelte";

  let { onComplete }: { onComplete: () => void } = $props();

  const labels = ONBOARDING_STEPS.map((step) => step.label);
  let step = $state(0);
  let account = $state<Account | null>(null);
  let skipped = $state(false);
  let viewport: HTMLDivElement;

  async function goTo(next: number) {
    if (next < 0 || next >= labels.length) return;
    step = next;
    await tick();
    viewport?.querySelector<HTMLElement>(".step-panel.active h1")?.focus({ preventScroll: true });
    if (viewport) viewport.scrollLeft = 0;
    setTimeout(() => {
      window.dispatchEvent(new Event("auramail:redraw-backdrop"));
    }, 460);
  }

  function connected(nextAccount: Account) {
    account = nextAccount;
    skipped = false;
    void goTo(5);
  }

  function skipAccount() {
    account = null;
    skipped = true;
    void goTo(5);
  }

  function finish() {
    markOnboardingComplete();
    onComplete();
  }
</script>

<div class="onboarding">
  <div class="preview-side">
    <OnboardingPreview label={labels[step]} />
  </div>
  <div class="step-side">
    <div class="progress" aria-label={`Step ${step + 1} of ${labels.length}`}>
      {#each labels as label, index (label)}
        <span class="progress-segment" class:active={index <= step}></span>
      {/each}
    </div>
    <div class="step-viewport" bind:this={viewport}>
      <div class="step-track" style={`transform: translateX(-${step * 100}%)`}>
        <section class="step-panel" class:active={step === 0} inert={step !== 0} aria-hidden={step !== 0}>
          <WelcomeStep onNext={() => void goTo(1)} />
        </section>
        <section class="step-panel" class:active={step === 1} inert={step !== 1} aria-hidden={step !== 1}>
          <AccessibilityStep onBack={() => void goTo(0)} onNext={() => void goTo(2)} />
        </section>
        <section class="step-panel" class:active={step === 2} inert={step !== 2} aria-hidden={step !== 2}>
          <AppearanceStep onBack={() => void goTo(1)} onNext={() => void goTo(3)} />
        </section>
        <section class="step-panel" class:active={step === 3} inert={step !== 3} aria-hidden={step !== 3}>
          <AiStep onBack={() => void goTo(2)} onNext={() => void goTo(4)} />
        </section>
        <section class="step-panel" class:active={step === 4} inert={step !== 4} aria-hidden={step !== 4}>
          <AccountStep onBack={() => void goTo(3)} onConnected={connected} onSkip={skipAccount} />
        </section>
        <section class="step-panel" class:active={step === 5} inert={step !== 5} aria-hidden={step !== 5}>
          <ReadyStep
            {account}
            {skipped}
            onBack={skipped ? () => void goTo(4) : undefined}
            onFinish={finish}
          />
        </section>
      </div>
    </div>
  </div>
</div>

<style>
  .onboarding {
    position: fixed;
    z-index: 30;
    inset: 0;
    display: grid;
    grid-template-columns: minmax(0, 0.72fr) minmax(0, 1fr);
    width: 100%;
    height: 100vh;
    height: 100dvh;
    overflow: hidden;
    background-color: var(--bg);
    background-image: var(--glass-backdrop-image);
    background-position: 0 0;
    background-repeat: no-repeat;
    background-size: 100% 100%;
    image-rendering: pixelated;
  }
  .preview-side {
    min-width: 0;
    padding: 18px;
    border-right: 1px solid color-mix(in srgb, var(--text) 12%, transparent);
    background: color-mix(in srgb, var(--bg) 10%, transparent);
    transition: background 240ms ease;
  }
  .step-side {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: 24px clamp(20px, 4vw, 54px) 20px;
    background: color-mix(in srgb, var(--bg) 24%, transparent);
  }
  .progress {
    display: flex;
    gap: 5px;
    width: min(100%, 620px);
    flex: none;
  }
  .progress-segment {
    height: 3px;
    flex: 1;
    border-radius: 99px;
    background: color-mix(in srgb, var(--border) 70%, transparent);
    transition: background 220ms ease;
  }
  .progress-segment.active { background: var(--accent); }
  .step-viewport {
    min-height: 0;
    flex: 1;
    overflow: hidden;
    overflow: clip;
    margin-top: 22px;
  }
  .onboarding,
  .onboarding * {
    scrollbar-width: none;
  }
  .onboarding *::-webkit-scrollbar {
    display: none;
  }
  .step-track {
    display: flex;
    width: 100%;
    height: 100%;
    transition: transform 420ms cubic-bezier(0.22, 0.8, 0.2, 1);
  }
  .step-panel {
    display: flex;
    flex: 0 0 100%;
    min-width: 0;
    height: 100%;
    overflow-y: auto;
    padding: 8px 12px 8px 8px;
  }
  .step-panel > :global(*) {
    width: min(100%, 620px);
    margin: auto 0;
  }
  @media (max-width: 899px) {
    .onboarding { grid-template-columns: minmax(0, 0.64fr) minmax(0, 1fr); }
    .preview-side { padding: 12px; }
  }
  @media (max-width: 639px) {
    .onboarding {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: 136px minmax(0, 1fr);
    }
    .preview-side {
      padding: 8px;
      border-right: 0;
      border-bottom: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
      overflow: hidden;
    }
    .step-side { padding: 16px 18px 14px; }
  }
</style>
