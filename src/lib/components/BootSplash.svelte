<script lang="ts">
  let { leaving = false }: { leaving?: boolean } = $props();
</script>

<div
  class="boot-splash"
  class:leaving
  role="status"
  aria-label="Starting AuraMail"
>
  <span class="halo" aria-hidden="true"></span>
  <span class="mark"><img src="/aura_mail_mark.svg" alt="" /></span>
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
    gap: 9px;
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
  .boot-splash.leaving .halo,
  .boot-splash.leaving .mark,
  .boot-splash.leaving strong,
  .boot-splash.leaving .tagline,
  .boot-splash.leaving .progress,
  .boot-splash.leaving .progress i {
    animation-play-state: paused;
  }
  .halo {
    position: absolute;
    width: min(44vmin, 320px);
    height: min(44vmin, 320px);
    border: 1px solid color-mix(in srgb, var(--accent) 18%, transparent);
    border-radius: 50%;
    box-shadow:
      0 0 0 36px color-mix(in srgb, var(--accent) 3%, transparent),
      0 0 90px color-mix(in srgb, var(--accent) 13%, transparent);
    opacity: 0;
    animation:
      splash-halo 2.6s cubic-bezier(0.22, 0.8, 0.2, 1) both,
      splash-breathe 5s 2.6s ease-in-out infinite;
  }
  .mark {
    position: relative;
    display: grid;
    place-items: center;
    width: 150px;
    height: 150px;
    animation: splash-mark 1.8s cubic-bezier(0.16, 0.84, 0.24, 1) both;
  }
  img {
    width: 84px;
    height: 84px;
    filter: drop-shadow(0 10px 28px color-mix(in srgb, var(--accent) 26%, transparent));
  }
  strong {
    position: relative;
    color: var(--text-strong);
    font-size: 19px;
    font-weight: 680;
    letter-spacing: -0.025em;
    animation: splash-copy 1.5s 320ms cubic-bezier(0.22, 0.8, 0.2, 1) both;
  }
  .tagline {
    position: relative;
    color: var(--muted);
    font-size: 10.5px;
    letter-spacing: 0.01em;
    animation: splash-copy 1.5s 520ms cubic-bezier(0.22, 0.8, 0.2, 1) both;
  }
  .progress {
    position: relative;
    width: 92px;
    height: 2px;
    margin-top: 12px;
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
  @keyframes splash-mark {
    0% { opacity: 0; transform: translateY(12px) scale(0.82); }
    55% { opacity: 1; }
    100% { opacity: 1; transform: translateY(0) scale(1); }
  }
  @keyframes splash-halo {
    0% { opacity: 0; transform: scale(0.74); }
    46% { opacity: 0.9; }
    100% { opacity: 0.6; transform: scale(1); }
  }
  @keyframes splash-breathe {
    0%, 100% { opacity: 0.6; transform: scale(1); }
    50% { opacity: 0.78; transform: scale(1.02); }
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
