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
    <span class="ring ring-1"></span>
    <span class="ring ring-2"></span>
    <span class="ring ring-3"></span>
    <span class="ring ring-4"></span>
    <svg class="envelope" viewBox="0 0 280 280" aria-hidden="true">
      <g transform="translate(10 0)">
        <polyline points="110,130 130,146 150,130" />
        <path
          d="M116 126 L144 126 C147.866 126 151 129.134 151 133 L151 149 C151 152.866 147.866 156 144 156 L116 156 C112.134 156 109 152.866 109 149 L109 133 C109 129.134 112.134 126 116 126 Z"
        />
      </g>
    </svg>
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
  .boot-splash.leaving .envelope,
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
    width: min(48vmin, 340px);
    aspect-ratio: 1;
    margin-bottom: clamp(16px, 2.8vmin, 28px);
    animation: splash-aura-in 900ms cubic-bezier(0.22, 0.8, 0.2, 1) both;
  }
  /* The four rings are the logo's circles (r 96 / 76 / 58 / 40 in a 280 box),
     rendered as separate layers so the breath stays on the compositor while
     the app is still booting. */
  .ring {
    position: absolute;
    inset: 0;
    margin: auto;
    border-radius: 50%;
    background: var(--accent);
    will-change: transform, opacity;
    animation-name: ring-breathe, ring-enter;
    animation-duration: 3s, 700ms;
    animation-timing-function: ease-in-out, cubic-bezier(0.16, 0.84, 0.24, 1);
    animation-iteration-count: infinite, 1;
    animation-fill-mode: none, backwards;
  }
  .ring-1 {
    width: 68.571%;
    height: 68.571%;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    --ring-in-delay: 360ms;
    --ring-breath-delay: 1660ms;
  }
  .ring-2 {
    width: 54.286%;
    height: 54.286%;
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    --ring-in-delay: 240ms;
    --ring-breath-delay: 1480ms;
  }
  .ring-3 {
    width: 41.429%;
    height: 41.429%;
    background: color-mix(in srgb, var(--accent) 24%, transparent);
    --ring-in-delay: 120ms;
    --ring-breath-delay: 1300ms;
  }
  .ring-4 {
    width: 28.571%;
    height: 28.571%;
    background: color-mix(in srgb, var(--accent) 74%, transparent);
    --ring-in-delay: 0ms;
    --ring-breath-delay: 1120ms;
  }
  .ring {
    animation-delay: var(--ring-breath-delay), var(--ring-in-delay);
  }
  .envelope {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    color: var(--text-strong);
    will-change: opacity;
    animation-name: envelope-breathe, envelope-enter;
    animation-duration: 3s, 700ms;
    animation-timing-function: ease-in-out, cubic-bezier(0.16, 0.84, 0.24, 1);
    animation-iteration-count: infinite, 1;
    animation-fill-mode: none, backwards;
    animation-delay: 1120ms, 0ms;
  }
  .envelope polyline,
  .envelope path {
    fill: none;
    stroke: currentColor;
    stroke-width: 3.5;
    stroke-linecap: round;
    stroke-linejoin: round;
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
  .boot-splash.reduced-motion .envelope,
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
    .envelope,
    strong,
    .tagline,
    .progress,
    .progress i {
      transition: none;
      animation: none;
    }
  }
  @keyframes splash-aura-in {
    0% { opacity: 0; transform: scale(0.98); }
    100% { opacity: 1; transform: scale(1); }
  }
  @keyframes ring-enter {
    0% { opacity: 0; transform: scale(0.82); }
    100% { opacity: 1; transform: scale(1); }
  }
  @keyframes ring-breathe {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.55; transform: scale(1.05); }
  }
  @keyframes envelope-enter {
    0% { opacity: 0; }
    100% { opacity: 1; }
  }
  @keyframes envelope-breathe {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.55; }
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
