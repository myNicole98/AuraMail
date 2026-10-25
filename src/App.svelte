<script lang="ts">
  import AppShell from "./lib/components/AppShell.svelte";
  import ComposeWindow from "./lib/components/ComposeWindow.svelte";

  const composeWindow =
    typeof window !== "undefined" &&
    new URLSearchParams(window.location.search).get("window") === "compose";
</script>

{#if composeWindow}
  <ComposeWindow />
{:else}
  <AppShell />
{/if}
