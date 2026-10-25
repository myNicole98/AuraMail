<script lang="ts">
  let {
    leaving = false,
    reducedMotion = false
  }: { leaving?: boolean; reducedMotion?: boolean } = $props();
</script>

<div
  class="boot-splash"
  class:leaving
  class:reduced-motion={reducedMotion}
  role="status"
  aria-label="Starting AuraMail"
>
  <div class="aura" aria-hidden="true">
    <span class="aura-glow"></span>
    <span class="ring ring-outer"></span>
    <span class="ring ring-middle"></span>
    <span class="ring ring-inner"></span>
    <span class="ring ring-core"></span>
    <span class="mark"><img src="/aura_mail_mark.svg" alt="" /></span>
  </div>
  <strong>AuraMail</strong>
  <span class="tagline">Preparing your inbox…</span>
  <span class="progress" aria-hidden="true"><i></i></span>
</div>

<style>
  .boot-splash {
    position: fixed;
    z-index: 40;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0;
    width: 100%;
    height: 100vh;
    height: 100dvh;
    background: color-mix(in srgb, var(--bg) 20%, transparent);
    color: var(--text);
    pointer-events: none;
    opacity: 1;
    transition: opacity 700ms cubic-bezier(0.22, 0.8, 0.2, 1);
  }
  .boot-splash.leaving {
    opacity: 0;
  }
  .boot-splash.leaving .aura,
  .boot-splash.leaving .ring,
  .boot-splash.leaving .mark,
  .boot-splash.leaving strong,
  .boot-splash.leaving .tagline,
  .boot-splash.leaving .progress,
  .boot-splash.leaving .progress i {
    animation-play-state: paused;
  }
  .aura {
    position: relative;
    display: grid;
    place-items: center;
    width: min(62vmin, 480px);
    aspect-ratio: 1;
    margin-bottom: clamp(14px, 2.4vmin, 26px);
    animation: splash-aura-in 1.3s cubic-bezier(0.16, 0.84, 0.24, 1) both;
  }
  .aura-glow {
    position: absolute;
    width: 54%;
    aspect-ratio: 1;
    border-radius: 50%;
    background: radial-gradient(
      circle,
      color-mix(in srgb, var(--accent) 24%, transparent) 0%,
      color-mix(in srgb, var(--info) 10%, transparent) 38%,
      transparent 72%
    );
    filter: blur(10px);
    opacity: 0.9;
  }
  .ring {
    position: absolute;
    inset: 0;
    box-sizing: border-box;
    margin: auto;
    border-radius: 50%;
    opacity: 0.55;
    transform: scale(1);
    will-change: transform, opacity;
  }
  .ring-outer {
    width: 100%;
    height: 100%;
    border: 1px solid color-mix(in srgb, var(--accent) 22%, transparent);
    box-shadow:
      0 0 64px color-mix(in srgb, var(--accent) 11%, transparent),
      inset 0 0 54px color-mix(in srgb, var(--accent) 7%, transparent);
    animation: ring-breathe 6.4s -1.6s ease-in-out infinite;
  }
  .ring-middle {
    width: 79%;
    height: 79%;
    border: 1px solid color-mix(in srgb, var(--info) 26%, transparent);
    box-shadow:
      0 0 48px color-mix(in srgb, var(--info) 10%, transparent),
      inset 0 0 42px color-mix(in srgb, var(--info) 6%, transparent);
    animation: ring-breathe 5.6s -0.8s ease-in-out infinite;
  }
  .ring-inner {
    width: 60%;
    height: 60%;
    border: 1px solid color-mix(in srgb, var(--accent-alt) 28%, transparent);
    box-shadow:
      0 0 38px color-mix(in srgb, var(--accent-alt) 10%, transparent),
      inset 0 0 32px color-mix(in srgb, var(--accent-alt) 6%, transparent);
    animation: ring-breathe 6.8s -2.7s ease-in-out infinite;
  }
  .ring-core {
    width: 42%;
    height: 42%;
    border: 1px solid color-mix(in srgb, var(--accent) 34%, transparent);
    background: radial-gradient(
      circle,
      color-mix(in srgb, var(--accent) 12%, transparent) 0%,
      transparent 68%
    );
    box-shadow: 0 0 34px color-mix(in srgb, var(--accent) 12%, transparent);
    animation: ring-breathe 5.2s -3.4s ease-in-out infinite;
  }
  .mark {
    position: relative;
    z-index: 1;
    display: grid;
    place-items: center;
    width: clamp(148px, 28vmin, 210px);
    height: clamp(148px, 28vmin, 210px);
    animation: splash-mark 1.6s cubic-bezier(0.16, 0.84, 0.24, 1) both;
  }
  img {
    width: clamp(104px, 20vmin, 150px);
    height: auto;
    filter: drop-shadow(
      0 14px 34px color-mix(in srgb, var(--accent) 28%, transparent)
    );
  }
  strong {
    position: relative;
    color: var(--text-strong);
    font-size: clamp(20px, 2.6vmin, 24px);
    font-weight: 680;
    line-height: 1.1;
    letter-spacing: -0.03em;
    animation: splash-copy 1.5s 320ms cubic-bezier(0.22, 0.8, 0.2, 1) both;
  }
  .tagline {
    position: relative;
    margin-top: 7px;
    color: var(--muted);
    font-size: clamp(10.5px, 1.35vmin, 12px);
    letter-spacing: 0.01em;
    animation: splash-copy 1.5s 520ms cubic-bezier(0.22, 0.8, 0.2, 1) both;
  }
  .progress {
    position: relative;
    width: clamp(96px, 14vmin, 128px);
    height: 2px;
    margin-top: clamp(18px, 3vmin, 26px);
    overflow: hidden;
    border-radius: 99px;
    background: color-mix(in srgb, var(--border) 46%, transparent);
    animation: splash-copy 1.4s 760ms ease both;
  }
  .progress i {
    display: block;
    width: 38%;
    height: 100%;
    border-radius: inherit;
    background: color-mix(in srgb, var(--accent) 78%, transparent);
    animation: splash-progress 1.7s ease-in-out infinite;
  }
  .boot-splash.reduced-motion,
  .boot-splash.reduced-motion .aura,
  .boot-splash.reduced-motion .ring,
  .boot-splash.reduced-motion .mark,
  .boot-splash.reduced-motion strong,
  .boot-splash.reduced-motion .tagline,
  .boot-splash.reduced-motion .progress,
  .boot-splash.reduced-motion .progress i {
    transition: none;
    animation: none;
  }
  @media (prefers-reduced-motion: reduce) {
    .boot-splash,
    .aura,
    .ring,
    .mark,
    strong,
    .tagline,
    .progress,
    .progress i {
      transition: none;
      animation: none;
    }
  }
  @keyframes splash-aura-in {
    0% { opacity: 0; transform: scale(0.9); }
    100% { opacity: 1; transform: scale(1); }
  }
  @keyframes splash-mark {
    0% { opacity: 0; transform: translateY(14px) scale(0.84); }
    55% { opacity: 1; }
    100% { opacity: 1; transform: translateY(0) scale(1); }
  }
  @keyframes ring-breathe {
    0%, 100% { opacity: 0.42; transform: scale(0.985); }
    46% { opacity: 0.86; transform: scale(1.035); }
    72% { opacity: 0.58; transform: scale(1.008); }
  }
  @keyframes splash-copy {
    0% { opacity: 0; transform: translateY(8px); }
    100% { opacity: 1; transform: translateY(0); }
  }
  @keyframes splash-progress {
    0% { transform: translateX(-120%); }
    55% { transform: translateX(80%); }
    100% { transform: translateX(280%); }
  }
</style>
