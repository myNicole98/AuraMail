<script lang="ts">
  let {
    kicker,
    title,
    description
  }: { kicker: string; title: string; description: string } = $props();
</script>

<header class="step-header">
  <span>{kicker}</span>
  <h1 tabindex="-1">{title}</h1>
  <p>{description}</p>
</header>

<style>
  .step-header span {
    color: var(--accent);
    font-size: 9px;
    font-weight: 750;
    letter-spacing: 0.16em;
    text-transform: uppercase;
  }
  h1 {
    margin: 7px 0 5px;
    color: var(--text-strong);
    font-size: clamp(24px, 3vw, 34px);
    line-height: 1.05;
    letter-spacing: -0.035em;
    outline: none;
  }
  p {
    max-width: 46ch;
    margin: 0;
    color: var(--muted);
    font-size: 12px;
    line-height: 1.55;
  }
</style>
