<script lang="ts">
  import { api } from "../api";
  import { initials } from "../format";

  let {
    name,
    email,
    enabled
  }: {
    name: string;
    email: string;
    enabled: boolean;
  } = $props();

  let image = $state<string | null>(null);
  let requested = $state("");

  $effect(() => {
    const address = email.trim();
    if (!enabled || !address || requested === address) return;
    let cancelled = false;
    image = null;
    void api
      .getSenderAvatar(address)
      .then((url) => {
        if (cancelled) return;
        requested = address;
        image = url;
      })
      .catch(() => {
        if (!cancelled) requested = address;
      });
    return () => {
      cancelled = true;
    };
  });

  function imageFailed() {
    image = null;
  }
</script>

<span class="avatar" aria-hidden="true">
  {#if enabled && image}
    <img src={image} alt="" loading="lazy" decoding="async" onerror={imageFailed} />
  {:else}
    {initials(name, email)}
  {/if}
</span>

<style>
  .avatar {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex: none;
    overflow: hidden;
    border-radius: 50%;
    background: color-mix(in srgb, var(--accent) 22%, var(--bg-hover));
    color: var(--text-strong);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.03em;
  }
  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
