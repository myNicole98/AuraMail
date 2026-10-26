<script lang="ts">
  import type { Account } from "../../types";

  let {
    mode,
    account = null,
    spaceId,
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

<!-- Placeholder shell. Task 7 replaces this body with the Proton login state machine. -->
<div class="proton-signin" data-testid="proton-signin">
  <p class="proton-status" role="status">
    {mode === "reconnect"
      ? "Proton reconnect is coming soon."
      : "Proton sign-in is coming soon."}
  </p>
  <button type="button" class="ghost" onclick={onCancel}>Cancel</button>
</div>

<style>
  .proton-signin {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 8px 2px;
    text-align: center;
  }
  .proton-status {
    margin: 0;
    color: var(--text);
    font-size: 12px;
  }
  .ghost {
    padding: 8px 15px;
    border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    border-radius: 8px;
    color: var(--muted);
    font-size: 11px;
    font-weight: 650;
  }
</style>
