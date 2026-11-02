<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { initials } from "../format";

  let {
    name,
    email,
    enabled,
    size = 28
  }: {
    name: string;
    email: string;
    enabled: boolean;
    size?: number;
  } = $props();

  let image = $state<string | null>(null);
  let requested = $state("");
  let element = $state<HTMLSpanElement | null>(null);
  let visible = $state(false);

  // Only resolve avatars for rows that scroll into view.
  onMount(() => {
    if (typeof IntersectionObserver === "undefined") {
      visible = true;
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          visible = true;
          observer.disconnect();
        }
      },
      { rootMargin: "150px" }
    );
    if (element) observer.observe(element);
    return () => observer.disconnect();
  });

  $effect(() => {
    const address = email.trim();
    if (!enabled || !visible || !address || requested === address) return;
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

<span
  class="avatar"
  bind:this={element}
  style:--avatar-size={`${size}px`}
  aria-hidden="true"
>
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
    width: var(--avatar-size, 28px);
    height: var(--avatar-size, 28px);
    flex: none;
    overflow: hidden;
    border-radius: 50%;
    background: color-mix(in srgb, var(--accent) 22%, var(--bg-hover));
    color: var(--text-strong);
    font-size: calc(var(--avatar-size, 28px) * 0.36);
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
