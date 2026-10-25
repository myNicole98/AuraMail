<script lang="ts">
  import OAuthSignIn from "./OAuthSignIn.svelte";
  import type { Account } from "../../types";

  let {
    mode,
    account = null,
    spaceId = undefined,
    onCancel,
    onConnected
  }: {
    mode: "add" | "reconnect";
    account?: Account | null;
    spaceId?: number;
    onCancel: () => void;
    onConnected: (account: Account) => void;
  } = $props();
</script>

<OAuthSignIn
  provider="google"
  {mode}
  {account}
  {spaceId}
  {onCancel}
  {onConnected}
/>
