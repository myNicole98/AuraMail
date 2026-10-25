<script module lang="ts">
  // Preview data URLs live for the whole session so reopening a message or
  // scrolling the same attachment back into view never refetches it.
  const cache = new Map<number, string | null>();
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import { api } from "../api";
  import type { Attachment } from "../types";

  let {
    attachment,
    enabled
  }: { attachment: Attachment; enabled: boolean } = $props();

  let dataUrl = $state<string | null>(null);

  let candidate = $derived(
    enabled &&
      attachment.mime_type.startsWith("image/") &&
      attachment.mime_type !== "image/svg+xml"
  );

  $effect(() => {
    const id = attachment.id;
    const wantsPreview = candidate;
    let cancelled = false;

    untrack(() => {
      if (!wantsPreview) {
        dataUrl = null;
        return;
      }
      if (cache.has(id)) {
        dataUrl = cache.get(id) ?? null;
        return;
      }
      void (async () => {
        let value: string | null = null;
        try {
          const preview = await api.attachmentPreview(id);
          value = preview?.data_url ?? null;
        } catch {
          // Thumbnails are decorative: a failed preview keeps the chip intact
          // and must never surface an error.
          value = null;
        }
        cache.set(id, value);
        if (!cancelled) dataUrl = value;
      })();
    });

    return () => {
      cancelled = true;
    };
  });
</script>

{#if dataUrl}
  <img
    class="attachment-thumb"
    src={dataUrl}
    alt={`Preview of ${attachment.filename}`}
  />
{/if}

<style>
  .attachment-thumb {
    width: 34px;
    height: 34px;
    flex: none;
    border-radius: 5px;
    background: var(--bg-hover);
    object-fit: cover;
  }
</style>
