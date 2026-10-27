<script lang="ts">
  import AppShell from "./lib/components/AppShell.svelte";
  import ComposeWindow from "./lib/components/ComposeWindow.svelte";
  import { readWindowRoute } from "./lib/windowRoute";

  const route =
    typeof window !== "undefined"
      ? readWindowRoute(window.location.search, window.location.hash)
      : { compose: false, params: new URLSearchParams() };
  const composeWindow = route.compose;
</script>

{#if composeWindow}
  <ComposeWindow />
{:else}
  <AppShell />
{/if}
