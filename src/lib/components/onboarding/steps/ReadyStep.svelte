<script lang="ts">
  import { Check, Inbox, Sparkles } from "lucide-svelte";
  import type { Account } from "../../../types";
  import StepHeader from "../StepHeader.svelte";

  let {
    account,
    skipped,
    onBack,
    onFinish
  }: {
    account: Account | null;
    skipped: boolean;
    onBack?: () => void;
    onFinish: () => void;
  } = $props();
</script>

<div class="ready">
  <StepHeader
    kicker="Ready"
    title="You're all set."
    description={account
      ? "Your inbox will sync quietly in the background."
      : "Add an account anytime in Settings."}
  />
  {#if account}
    <div class="status">
      <span class="icon"><Check size={16} /></span>
      <div>
        <strong>{account.email}</strong>
        <small>Verified and ready</small>
      </div>
    </div>
  {/if}
  <div class="promise">
    <Sparkles size={14} />
    <span>Your setup choices are saved.</span>
  </div>
  <div class="actions">
    {#if onBack}
      <button class="ghost" type="button" onclick={onBack}>Back</button>
    {:else}
      <span></span>
    {/if}
    <button class="primary" type="button" onclick={onFinish}>
      <Inbox size={15} />
      Open AuraMail
    </button>
  </div>
</div>

<style>
  .ready { display: flex; flex-direction: column; gap: 20px; }
  .status {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 14px;
    border: 1px solid color-mix(in srgb, var(--accent) 30%, var(--border));
    border-radius: 12px;
    background: color-mix(in srgb, var(--accent) 7%, var(--bg-raised));
  }
  .icon {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--success) 16%, transparent);
    color: var(--success);
  }
  .status strong { display: block; color: var(--text-strong); font-size: 11px; }
  .status small { display: block; margin-top: 2px; color: var(--muted); font-size: 9px; }
  .promise { display: flex; align-items: center; gap: 8px; color: var(--muted); font-size: 10px; }
  .promise :global(svg) { color: var(--accent); }
  .actions { display: flex; align-items: center; justify-content: space-between; }
  .ghost,
  .primary {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 10px 17px;
    border-radius: 9px;
    font-size: 10.5px;
    font-weight: 650;
  }
  .ghost { border: 1px solid color-mix(in srgb, var(--border) 80%, transparent); color: var(--muted); }
  .primary {
    background: var(--accent);
    color: var(--bg);
    box-shadow: 0 8px 24px color-mix(in srgb, var(--accent) 24%, transparent);
  }
</style>
