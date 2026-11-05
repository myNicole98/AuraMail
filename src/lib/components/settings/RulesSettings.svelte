<script lang="ts">
  import { onMount } from "svelte";
  import {
    ArrowDown,
    ArrowUp,
    Check,
    ChevronDown,
    ChevronUp,
    GripVertical,
    Pencil,
    Play,
    Plus,
    Trash2,
    X
  } from "lucide-svelte";
  import { api } from "../../api";
  import { app } from "../../stores/app.svelte";
  import ToggleSwitch from "../ToggleSwitch.svelte";
  import ThemedSelect from "../ThemedSelect.svelte";
  import TagInput from "./TagInput.svelte";
  import type {
    Folder,
    Label,
    Rule,
    RuleAction,
    RuleCondition,
    RuleInput,
    RulePriority,
    RuleSensitivity
  } from "../../types";
  import { errorMessage } from "./errors";

  type ConditionKind = RuleCondition["kind"];
  type ActionKind = RuleAction["kind"];

  let {
    ruleSeed = null,
    requestNonce = 0
  }: {
    ruleSeed?: { accountId?: number; conditions: RuleCondition[] } | null;
    requestNonce?: number;
  } = $props();

  const CONDITION_GROUPS: { group: string; options: { kind: ConditionKind; label: string }[] }[] = [
    {
      group: "People",
      options: [
        { kind: "fromContains", label: "From" },
        { kind: "toContains", label: "To" },
        { kind: "ccContains", label: "Cc" },
        { kind: "onBehalfOf", label: "Received on behalf of others" },
        { kind: "myNameInTo", label: "My name is in the To box" },
        { kind: "myNameInCc", label: "My name is in the Cc box" },
        { kind: "myNameInToOrCc", label: "My name is in the To or Cc box" },
        { kind: "myNameNotInTo", label: "My name is not in the To box" },
        { kind: "onlyRecipient", label: "I am the only recipient" },
        { kind: "mentionedInBody", label: "I am mentioned in the message" }
      ]
    },
    {
      group: "Subject",
      options: [
        { kind: "subjectContains", label: "Subject" },
        { kind: "subjectOrBodyContains", label: "Subject or body" }
      ]
    },
    {
      group: "Keywords",
      options: [
        { kind: "bodyContains", label: "Body" },
        { kind: "senderAddressContains", label: "Sender address" },
        { kind: "recipientAddressContains", label: "Recipient address" },
        { kind: "headerContains", label: "Message header" }
      ]
    },
    {
      group: "Marked with",
      options: [
        { kind: "importance", label: "Importance" },
        { kind: "sensitivity", label: "Sensitivity" },
        { kind: "externalSender", label: "External sender" }
      ]
    },
    {
      group: "Message includes",
      options: [
        { kind: "hasFlag", label: "Flagged message" },
        { kind: "messageType", label: "Message type" },
        { kind: "hasAttachment", label: "Has an attachment" },
        { kind: "hasLabel", label: "Label" },
        { kind: "hasAnyLabel", label: "Any label" }
      ]
    }
  ];

  const ACTION_GROUPS: { group: string; options: { kind: ActionKind; label: string }[] }[] = [
    {
      group: "Organise",
      options: [
        { kind: "moveTo", label: "Move to folder" },
        { kind: "copyTo", label: "Copy to folder" },
        { kind: "delete", label: "Delete" },
        { kind: "permanentDelete", label: "Permanently delete" }
      ]
    },
    {
      group: "Mark message",
      options: [
        { kind: "markRead", label: "Mark as read" },
        { kind: "markJunk", label: "Mark as junk" },
        { kind: "setImportance", label: "Assign importance" },
        { kind: "addLabel", label: "Add label" },
        { kind: "clearLabels", label: "Clear labels" },
        { kind: "setFlag", label: "Flag message" },
        { kind: "clearFlag", label: "Clear flag" },
        { kind: "completeFlag", label: "Mark flag complete" }
      ]
    },
    {
      group: "Forward",
      options: [
        { kind: "forwardTo", label: "Forward to" },
        { kind: "forwardAsAttachment", label: "Forward as attachment to" },
        { kind: "redirectTo", label: "Redirect to" }
      ]
    },
    {
      group: "Notify",
      options: [
        { kind: "notifyDesktop", label: "Show desktop notification" },
        { kind: "playSound", label: "Play a sound" }
      ]
    }
  ];

  const CONDITION_SELECT_GROUPS = CONDITION_GROUPS.map((group) => ({
    label: group.group,
    options: group.options.map((option) => ({ value: option.kind, label: option.label }))
  }));
  const ACTION_SELECT_GROUPS = ACTION_GROUPS.map((group) => ({
    label: group.group,
    options: group.options.map((option) => ({ value: option.kind, label: option.label }))
  }));

  function defaultCondition(kind: ConditionKind): RuleCondition {
    switch (kind) {
      case "fromContains":
      case "toContains":
      case "ccContains":
      case "subjectContains":
      case "subjectOrBodyContains":
      case "bodyContains":
      case "senderAddressContains":
      case "recipientAddressContains":
        return { kind, values: [] };
      case "headerContains":
        return { kind, name: "", values: [] };
      case "importance":
        return { kind, value: "high" };
      case "sensitivity":
        return { kind, value: "private" };
      case "messageType":
        return { kind, value: "invitation" };
      case "hasLabel":
        return { kind, label_id: labels[0]?.id ?? 0 };
      default:
        return { kind } as RuleCondition;
    }
  }

  function defaultAction(kind: ActionKind): RuleAction {
    switch (kind) {
      case "moveTo":
      case "copyTo":
        return { kind, folder_id: foldersFor(accountId)[0]?.id ?? 0 };
      case "setImportance":
        return { kind, value: "high" };
      case "addLabel":
        return { kind, label_id: labels[0]?.id ?? 0 };
      case "forwardTo":
      case "forwardAsAttachment":
      case "redirectTo":
        return { kind, address: "" };
      default:
        return { kind } as RuleAction;
    }
  }

  let accountId = $state<number>(app.accounts[0]?.id ?? 0);
  let rules = $state<Rule[]>([]);
  let labels = $state<Label[]>([]);
  let editing = $state<{ id: number | null; draft: RuleInput } | null>(null);
  let expanded = $state<number | null>(null);
  let dragging = $state<Rule | null>(null);
  let runningRule = $state<number | null>(null);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let search = $state("");
  let nameMissing = $state(false);

  const accountRules = $derived(rules.filter((rule) => rule.account_id === accountId));
  const visibleRules = $derived(
    accountRules.filter((rule) =>
      rule.name.toLowerCase().includes(search.trim().toLowerCase())
    )
  );
  const accountFolders = $derived(foldersFor(accountId));
  const accountOptions = $derived(
    app.accounts.map((account) => ({ value: String(account.id), label: account.email }))
  );

  // Suggestions for address-like condition values: the user's own accounts and
  // everyone already visible in the message list.
  const suggestions = $derived.by(() => {
    const seen = new Set<string>();
    for (const account of app.accounts) {
      if (account.email) seen.add(account.email);
      if (account.identity) seen.add(account.identity);
    }
    for (const message of app.messages) {
      if (message.from_addr) seen.add(message.from_addr);
      if (message.from_name.trim()) seen.add(message.from_name);
    }
    return [...seen];
  });

  function foldersFor(id: number): Folder[] {
    return app.folders.filter(
      (folder) =>
        folder.account_id === id &&
        folder.role !== "drafts" &&
        folder.role !== "all"
    );
  }

  async function load() {
    try {
      [rules, labels] = await Promise.all([api.listRules(null), api.listLabels()]);
      app.labels = labels;
    } catch (caught) {
      error = errorMessage(caught);
    }
  }

  onMount(() => {
    void load();
  });

  // Opened from the message context menu ("Create rule for this mail").
  $effect(() => {
    requestNonce;
    if (!ruleSeed) return;
    const target =
      ruleSeed.accountId && app.accounts.some((account) => account.id === ruleSeed.accountId)
        ? ruleSeed.accountId
        : accountId;
    accountId = target;
    editing = {
      id: null,
      draft: {
        account_id: target,
        name: "New rule",
        enabled: true,
        stop_processing: false,
        conditions: ruleSeed.conditions.map((condition) => ({ ...condition })),
        exceptions: [],
        actions: []
      }
    };
  });

  function noticeOnce(message: string) {
    notice = message;
    setTimeout(() => {
      if (notice === message) notice = null;
    }, 3200);
  }

  function blankRule(): RuleInput {
    return {
      account_id: accountId,
      name: "",
      enabled: true,
      stop_processing: false,
      conditions: [],
      exceptions: [],
      actions: []
    };
  }

  function startNew() {
    nameMissing = false;
    editing = { id: null, draft: blankRule() };
  }

  function startEdit(rule: Rule) {
    nameMissing = false;
    editing = {
      id: rule.id,
      draft: {
        account_id: rule.account_id,
        name: rule.name,
        enabled: rule.enabled,
        stop_processing: rule.stop_processing,
        conditions: $state.snapshot(rule.conditions) as RuleCondition[],
        exceptions: $state.snapshot(rule.exceptions) as RuleCondition[],
        actions: $state.snapshot(rule.actions) as RuleAction[]
      }
    };
  }

  function closeEditor() {
    editing = null;
    error = null;
    nameMissing = false;
  }

  async function save() {
    if (!editing) return;
    if (!editing.draft.name.trim()) {
      nameMissing = true;
      return;
    }
    try {
      await api.saveRule(editing.id, editing.draft);
      editing = null;
      error = null;
      await load();
      noticeOnce("Rule saved");
    } catch (caught) {
      error = errorMessage(caught);
    }
  }

  async function remove(rule: Rule) {
    try {
      await api.deleteRule(rule.id);
      expanded = null;
      await load();
      noticeOnce("Rule deleted");
    } catch (caught) {
      error = errorMessage(caught);
    }
  }

  async function move(rule: Rule, delta: number) {
    const ordered = accountRules.map((item) => item.id);
    const index = ordered.indexOf(rule.id);
    const next = index + delta;
    if (index < 0 || next < 0 || next >= ordered.length) return;
    [ordered[index], ordered[next]] = [ordered[next], ordered[index]];
    try {
      await api.reorderRules(rule.account_id, ordered);
      await load();
    } catch (caught) {
      error = errorMessage(caught);
    }
  }

  async function toggle(rule: Rule, enabled: boolean) {
    try {
      await api.saveRule(rule.id, {
        account_id: rule.account_id,
        name: rule.name,
        enabled,
        stop_processing: rule.stop_processing,
        conditions: rule.conditions,
        exceptions: rule.exceptions,
        actions: rule.actions
      });
      await load();
    } catch (caught) {
      error = errorMessage(caught);
    }
  }

  async function runRule(rule: Rule) {
    if (runningRule !== null) return;
    runningRule = rule.id;
    try {
      const result = await api.runRules(rule.account_id, null, rule.id);
      const parts = [`${result.matched} matched`];
      if (result.moved > 0) parts.push(`${result.moved} moved`);
      if (result.failed > 0) parts.push(`${result.failed} failed`);
      noticeOnce(`${rule.name}: ${parts.join(", ")}`);
      void app.reloadMessages();
    } catch (caught) {
      error = errorMessage(caught);
    } finally {
      runningRule = null;
    }
  }

  function onDragStart(event: DragEvent, rule: Rule) {
    dragging = rule;
    if (event.dataTransfer) {
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData("text/plain", String(rule.id));
    }
  }

  async function onDrop(event: DragEvent, target: Rule) {
    event.preventDefault();
    const source = dragging;
    dragging = null;
    if (!source || source.id === target.id) return;
    if (source.account_id !== target.account_id) return;
    const ordered = accountRules.map((item) => item.id);
    const from = ordered.indexOf(source.id);
    const to = ordered.indexOf(target.id);
    if (from < 0 || to < 0) return;
    ordered.splice(from, 1);
    ordered.splice(to, 0, source.id);
    try {
      await api.reorderRules(source.account_id, ordered);
      await load();
    } catch (caught) {
      error = errorMessage(caught);
    }
  }

  function addCondition(kind: string, target: "conditions" | "exceptions") {
    if (!editing || !kind) return;
    const next = defaultCondition(kind as ConditionKind);
    if (target === "conditions") {
      editing.draft.conditions = [...editing.draft.conditions, next];
    } else {
      editing.draft.exceptions = [...editing.draft.exceptions, next];
    }
  }

  function changeCondition(
    kind: string,
    target: "conditions" | "exceptions",
    index: number
  ) {
    if (!editing || !kind) return;
    const next = defaultCondition(kind as ConditionKind);
    if (target === "conditions") {
      editing.draft.conditions = editing.draft.conditions.map((item, i) =>
        i === index ? next : item
      );
    } else {
      editing.draft.exceptions = editing.draft.exceptions.map((item, i) =>
        i === index ? next : item
      );
    }
  }

  function removeAt(target: "conditions" | "exceptions" | "actions", index: number) {
    if (!editing) return;
    const draft = editing.draft;
    if (target === "actions") {
      draft.actions = draft.actions.filter((_, i) => i !== index);
    } else if (target === "exceptions") {
      draft.exceptions = draft.exceptions.filter((_, i) => i !== index);
    } else {
      draft.conditions = draft.conditions.filter((_, i) => i !== index);
    }
  }

  function addAction(kind: string) {
    if (!editing || !kind) return;
    editing.draft.actions = [...editing.draft.actions, defaultAction(kind as ActionKind)];
  }

  function folderName(id: number): string {
    return app.folders.find((folder) => folder.id === id)?.name ?? "folder";
  }

  function labelName(id: number): string {
    return labels.find((label) => label.id === id)?.name ?? "label";
  }

  function quote(values: string[]): string {
    if (values.length === 0) return "…";
    if (values.length === 1) return `"${values[0]}"`;
    return values.map((value) => `"${value}"`).join(" or ");
  }

  function describeCondition(condition: RuleCondition): string {
    switch (condition.kind) {
      case "fromContains":
        return `the sender is ${quote(condition.values)}`;
      case "toContains":
        return `the recipient is ${quote(condition.values)}`;
      case "ccContains":
        return `the Cc line is ${quote(condition.values)}`;
      case "onBehalfOf":
        return "the message was received on behalf of others";
      case "myNameInTo":
        return "my address is in the To box";
      case "myNameInCc":
        return "my address is in the Cc box";
      case "myNameInToOrCc":
        return "my address is in the To or Cc box";
      case "myNameNotInTo":
        return "my address is not in the To box";
      case "onlyRecipient":
        return "I am the only recipient";
      case "mentionedInBody":
        return "I am mentioned in the message";
      case "subjectContains":
        return `the subject contains ${quote(condition.values)}`;
      case "subjectOrBodyContains":
        return `the subject or body contains ${quote(condition.values)}`;
      case "bodyContains":
        return `the body contains ${quote(condition.values)}`;
      case "senderAddressContains":
        return `the sender address contains ${quote(condition.values)}`;
      case "recipientAddressContains":
        return `the recipient address contains ${quote(condition.values)}`;
      case "headerContains":
        return `the header ${condition.name} contains ${quote(condition.values)}`;
      case "importance":
        return `the importance is ${condition.value}`;
      case "sensitivity":
        return `the sensitivity is ${condition.value}`;
      case "externalSender":
        return "the sender is external";
      case "hasFlag":
        return "the message is flagged";
      case "messageType":
        return `the message is a ${condition.value}`;
      case "hasAttachment":
        return "the message has an attachment";
      case "hasLabel":
        return `the message has the label "${labelName(condition.label_id)}"`;
      case "hasAnyLabel":
        return "the message has any label";
    }
  }

  function describeAction(action: RuleAction): string {
    switch (action.kind) {
      case "moveTo":
        return `move it to "${folderName(action.folder_id)}"`;
      case "copyTo":
        return `copy it to "${folderName(action.folder_id)}"`;
      case "delete":
        return "delete it";
      case "permanentDelete":
        return "delete it permanently";
      case "markRead":
        return "mark it as read";
      case "markJunk":
        return "mark it as junk";
      case "setImportance":
        return `set its importance to ${action.value}`;
      case "addLabel":
        return `add the label "${labelName(action.label_id)}"`;
      case "clearLabels":
        return "clear its labels";
      case "setFlag":
        return "flag it";
      case "clearFlag":
        return "clear its flag";
      case "completeFlag":
        return "mark its flag complete";
      case "forwardTo":
        return `forward it to ${action.address}`;
      case "forwardAsAttachment":
        return `forward it as an attachment to ${action.address}`;
      case "redirectTo":
        return `redirect it to ${action.address}`;
      case "notifyDesktop":
        return "show a desktop notification";
      case "playSound":
        return "play a sound";
    }
  }

  function describe(rule: Rule): string {
    if (rule.conditions.length === 0) {
      return "Any message matches this rule.";
    }
    const when = rule.conditions.map(describeCondition).join(" and ");
    const exceptions =
      rule.exceptions.length > 0
        ? `, unless ${rule.exceptions.map(describeCondition).join(" or ")}`
        : "";
    const then = rule.actions.map(describeAction).join(", then ");
    const stop = rule.stop_processing ? ", and stop processing more rules" : "";
    return `When ${when}${exceptions}: ${then}${stop}.`;
  }
</script>

<div class="rules">
  <header class="page-head">
    <h2>Rules</h2>
    <p>
      Rules tell AuraMail how to handle incoming mail. They are applied top to bottom,
      and the first match can stop the ones below it.
    </p>
  </header>

  {#if error}
    <p class="banner error">{error}</p>
  {/if}
  {#if notice}
    <p class="banner ok">{notice}</p>
  {/if}

  {#if app.accounts.length > 1}
    <div class="account-row">
      <span class="account-label">Rules for</span>
      <div class="account-select">
        <ThemedSelect
          value={String(accountId)}
          options={accountOptions}
          onchange={(value) => {
            accountId = Number(value);
            expanded = null;
          }}
          ariaLabel="Account"
        />
      </div>
    </div>
  {/if}

  <div class="toolbar">
    <input class="search" bind:value={search} placeholder="Search rules by name" />
    <button
      type="button"
      class="primary add"
      disabled={app.accounts.length === 0}
      onclick={startNew}
    >
      <Plus size={15} /> Add rule
    </button>
  </div>

  <div class="rules-table" role="group" aria-label="Rules">
    {#if visibleRules.length === 0}
      <p class="empty">
        {search.trim()
          ? "No rule matches that search."
          : "No rules yet. Add one to sort, label, or forward mail automatically."}
      </p>
    {:else}
      {#each visibleRules as rule, index (rule.id)}
        <div
          class="rule-row"
          role="group"
          aria-label={rule.name}
          class:off={!rule.enabled}
          class:open={expanded === rule.id}
          class:dragging={dragging?.id === rule.id}
          ondragover={(event) => event.preventDefault()}
          ondrop={(event) => void onDrop(event, rule)}
        >
          <span
            class="grip"
            draggable="true"
            role="button"
            tabindex="-1"
            aria-label={`Reorder ${rule.name}`}
            ondragstart={(event) => onDragStart(event, rule)}
            ondragend={() => (dragging = null)}
          >
            <GripVertical size={15} />
          </span>
          <span class="rule-name" title={rule.name}>{rule.name}</span>
          <span class="rule-toggle">
            <ToggleSwitch
              checked={rule.enabled}
              small
              label={`Enable ${rule.name}`}
              onchange={(value) => void toggle(rule, value)}
            />
          </span>
          <span class="rule-actions">
            <button
              type="button"
              class="icon"
              disabled={index === 0}
              onclick={() => void move(rule, -1)}
              aria-label={`Move ${rule.name} up`}
            >
              <ArrowUp size={15} />
            </button>
            <button
              type="button"
              class="icon"
              disabled={index === visibleRules.length - 1}
              onclick={() => void move(rule, 1)}
              aria-label={`Move ${rule.name} down`}
            >
              <ArrowDown size={15} />
            </button>
            <button
              type="button"
              class="icon"
              disabled={runningRule !== null}
              onclick={() => void runRule(rule)}
              aria-label={`Run ${rule.name} now`}
              title="Run this rule now"
            >
              <Play size={15} />
            </button>
            <button
              type="button"
              class="icon"
              aria-expanded={expanded === rule.id}
              aria-label={`${expanded === rule.id ? "Collapse" : "Expand"} ${rule.name}`}
              onclick={() => (expanded = expanded === rule.id ? null : rule.id)}
            >
              {#if expanded === rule.id}
                <ChevronUp size={15} />
              {:else}
                <ChevronDown size={15} />
              {/if}
            </button>
          </span>
        </div>
        {#if expanded === rule.id}
          <div class="rule-detail">
            <p>{describe(rule)}</p>
            <div class="rule-detail-actions">
              <button type="button" class="ghost" onclick={() => startEdit(rule)}>
                <Pencil size={14} /> Edit
              </button>
              <button type="button" class="ghost danger" onclick={() => void remove(rule)}>
                <Trash2 size={14} /> Delete
              </button>
            </div>
          </div>
        {/if}
      {/each}
    {/if}
  </div>
</div>

{#if editing}
  <div
    class="dialog-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) closeEditor();
    }}
  >
    <form
      class="rule-dialog"
      aria-label={editing.id === null ? "New rule" : "Edit rule"}
      onsubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <div class="dialog-head">
        <div>
          <strong>{editing.id === null ? "New rule" : "Edit rule"}</strong>
          <span>Applied to new mail in this account's inboxes.</span>
        </div>
        <button
          type="button"
          class="ghost icon"
          aria-label="Close rule editor"
          onclick={closeEditor}
        >
          <X size={18} />
        </button>
      </div>

      <div class="step">
        <span class="bullet">1</span>
        <div class="step-main">
          <label class="step-title" for="rule-name">Rule name</label>
          <input
            id="rule-name"
            class="step-input"
            class:invalid={nameMissing}
            bind:value={editing.draft.name}
            placeholder="Enter a rule name"
            oninput={() => (nameMissing = false)}
          />
          {#if nameMissing}
            <p class="step-error">Enter a name for this rule.</p>
          {/if}
        </div>
      </div>

      <div class="step">
        <span class="bullet">2</span>
        <div class="step-main">
          <p class="step-title">Add a condition</p>
          {#each editing.draft.conditions as condition, index (index)}
            <div class="rule-line">
              <div class="type-select">
                <ThemedSelect
                  groups={CONDITION_SELECT_GROUPS}
                  value={condition.kind}
                  onchange={(kind) => changeCondition(kind, "conditions", index)}
                  ariaLabel="Condition"
                />
              </div>
              {@render ConditionFields({ condition, labels, suggestions })}
              <button
                type="button"
                class="ghost icon remove"
                onclick={() => removeAt("conditions", index)}
                aria-label="Remove condition"
              >
                <X size={16} />
              </button>
            </div>
          {/each}
          {#if editing.draft.conditions.length === 0}
            <ThemedSelect
              value=""
              groups={CONDITION_SELECT_GROUPS}
              onchange={(value) => addCondition(value, "conditions")}
              ariaLabel="Select a condition"
              placeholder="Select a condition"
            />
          {:else}
            <button
              type="button"
              class="link"
              onclick={() => editing && addCondition("fromContains", "conditions")}
            >
              Add another condition
            </button>
          {/if}
        </div>
      </div>

      <div class="step">
        <span class="bullet">3</span>
        <div class="step-main">
          <p class="step-title">Add an action</p>
          {#each editing.draft.actions as action, index (index)}
            <div class="rule-line">
              <div class="type-select">
                <ThemedSelect
                  groups={ACTION_SELECT_GROUPS}
                  value={action.kind}
                  onchange={(kind) => {
                    if (!editing || !kind) return;
                    editing.draft.actions = editing.draft.actions.map((item, i) =>
                      i === index ? defaultAction(kind as ActionKind) : item
                    );
                  }}
                  ariaLabel="Action"
                />
              </div>
              {@render ActionFields({ action, labels, folders: accountFolders })}
              <button
                type="button"
                class="ghost icon remove"
                onclick={() => removeAt("actions", index)}
                aria-label="Remove action"
              >
                <X size={16} />
              </button>
            </div>
          {/each}
          {#if editing.draft.actions.length === 0}
            <ThemedSelect
              value=""
              groups={ACTION_SELECT_GROUPS}
              onchange={(value) => addAction(value)}
              ariaLabel="Select an action"
              placeholder="Select an action"
            />
          {:else}
            <button
              type="button"
              class="link"
              onclick={() => editing && addAction("moveTo")}
            >
              Add another action
            </button>
          {/if}
        </div>
      </div>

      <div class="step exceptions">
        <span class="bullet">4</span>
        <div class="step-main">
          <p class="step-title">Except if</p>
          {#each editing.draft.exceptions as condition, index (index)}
            <div class="rule-line">
              <div class="type-select">
                <ThemedSelect
                  groups={CONDITION_SELECT_GROUPS}
                  value={condition.kind}
                  onchange={(kind) => changeCondition(kind, "exceptions", index)}
                  ariaLabel="Exception"
                />
              </div>
              {@render ConditionFields({ condition, labels, suggestions })}
              <button
                type="button"
                class="ghost icon remove"
                onclick={() => removeAt("exceptions", index)}
                aria-label="Remove exception"
              >
                <X size={16} />
              </button>
            </div>
          {/each}
          <button
            type="button"
            class="link"
            onclick={() => editing && addCondition("subjectContains", "exceptions")}
          >
            Add an exception
          </button>
        </div>
      </div>

      <label class="check-row">
        <input type="checkbox" bind:checked={editing.draft.stop_processing} />
        <span>Stop processing more rules</span>
      </label>

      <div class="dialog-actions">
        <button type="button" class="ghost" onclick={closeEditor}>Cancel</button>
        <button type="submit" class="primary">
          <Check size={14} /> Save rule
        </button>
      </div>
    </form>
  </div>
{/if}

{#snippet ConditionFields({
  condition,
  labels: availableLabels,
  suggestions: hints
}: {
  condition: RuleCondition;
  labels: Label[];
  suggestions: string[];
})}
  {#if condition.kind === "headerContains"}
    <input class="inline-input" bind:value={condition.name} placeholder="Header name" />
    <TagInput
      values={condition.values}
      onchange={(values) => (condition.values = values)}
      suggestions={hints}
      placeholder="Value"
    />
  {:else if condition.kind === "importance"}
    <div class="value-select">
      <ThemedSelect
        value={condition.value}
        options={[
          { value: "high", label: "High" },
          { value: "normal", label: "Normal" },
          { value: "low", label: "Low" }
        ]}
        onchange={(value) => (condition.value = value as RulePriority)}
        ariaLabel="Importance"
      />
    </div>
  {:else if condition.kind === "sensitivity"}
    <div class="value-select">
      <ThemedSelect
        value={condition.value}
        options={[
          { value: "normal", label: "Normal" },
          { value: "personal", label: "Personal" },
          { value: "private", label: "Private" },
          { value: "confidential", label: "Confidential" }
        ]}
        onchange={(value) => (condition.value = value as RuleSensitivity)}
        ariaLabel="Sensitivity"
      />
    </div>
  {:else if condition.kind === "messageType"}
    <div class="value-select">
      <ThemedSelect
        value={condition.value}
        options={[
          { value: "normal", label: "Message" },
          { value: "invitation", label: "Meeting invitation" },
          { value: "receipt", label: "Read receipt" }
        ]}
        onchange={(value) => (condition.value = value as never)}
        ariaLabel="Message type"
      />
    </div>
  {:else if condition.kind === "hasLabel"}
    <div class="value-select">
      <ThemedSelect
        value={String(condition.label_id)}
        options={availableLabels.map((label) => ({
          value: String(label.id),
          label: label.name
        }))}
        onchange={(value) => (condition.label_id = Number(value))}
        ariaLabel="Label"
      />
    </div>
  {:else if "values" in condition}
    <TagInput
      values={condition.values}
      onchange={(values) => (condition.values = values)}
      suggestions={hints}
      placeholder={condition.kind === "fromContains" || condition.kind === "senderAddressContains"
        ? "Add an address or name"
        : "Add a word or phrase"}
    />
  {/if}
{/snippet}

{#snippet ActionFields({
  action,
  labels: availableLabels,
  folders
}: {
  action: RuleAction;
  labels: Label[];
  folders: Folder[];
})}
  {#if action.kind === "moveTo" || action.kind === "copyTo"}
    <div class="value-select">
      <ThemedSelect
        value={String(action.folder_id)}
        options={folders.map((folder) => ({ value: String(folder.id), label: folder.name }))}
        onchange={(value) => (action.folder_id = Number(value))}
        ariaLabel="Target folder"
      />
    </div>
  {:else if action.kind === "setImportance"}
    <div class="value-select">
      <ThemedSelect
        value={action.value}
        options={[
          { value: "high", label: "High" },
          { value: "normal", label: "Normal" },
          { value: "low", label: "Low" }
        ]}
        onchange={(value) => (action.value = value as RulePriority)}
        ariaLabel="Importance"
      />
    </div>
  {:else if action.kind === "addLabel"}
    <div class="value-select">
      <ThemedSelect
        value={String(action.label_id)}
        options={availableLabels.map((label) => ({
          value: String(label.id),
          label: label.name
        }))}
        onchange={(value) => (action.label_id = Number(value))}
        ariaLabel="Label"
      />
    </div>
  {:else if action.kind === "forwardTo" || action.kind === "forwardAsAttachment" || action.kind === "redirectTo"}
    <input class="inline-input" bind:value={action.address} placeholder="name@example.com" type="email" />
  {/if}
{/snippet}

<style>
  .rules {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: min(880px, 100%);
    margin: 0 auto;
  }
  .page-head h2 {
    margin: 0 0 6px;
    font-size: 1.15rem;
    color: var(--text-strong);
  }
  .page-head p {
    margin: 0;
    max-width: 62ch;
    color: var(--muted);
    font-size: 0.85rem;
    line-height: 1.5;
  }
  .banner {
    margin: 0;
    padding: 8px 12px;
    border-radius: 8px;
    font-size: 0.82rem;
  }
  .banner.error {
    background: color-mix(in srgb, var(--danger) 14%, transparent);
    color: var(--danger);
  }
  .banner.ok {
    background: color-mix(in srgb, var(--success) 14%, transparent);
    color: var(--success);
  }
  .account-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .account-label {
    color: var(--muted);
    font-size: 0.78rem;
  }
  .account-select {
    width: 240px;
  }
  .toolbar {
    display: flex;
    gap: 10px;
    align-items: stretch;
  }
  /* Svelte scopes the trailing compound with :where(), which has no
     specificity, so these must be forced past settings.css. */
  .rules .toolbar .search,
  .rules .toolbar .add {
    height: 36px !important;
    min-height: 36px !important;
    box-sizing: border-box;
  }
  .rules .toolbar .search {
    flex: 1 1 auto;
    min-width: 0;
  }
  .rules .toolbar .add {
    flex: 0 0 auto;
    min-width: 176px;
    padding: 0 22px !important;
    font-size: 0.84rem;
  }
  .rules-table {
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
    background: var(--bg-raised);
  }
  .rule-row {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr) 46px auto;
    align-items: center;
    gap: 10px;
    min-height: 50px;
    padding: 0 12px 0 8px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  }
  .rule-row:last-of-type {
    border-bottom: 0;
  }
  .rule-row.off .rule-name {
    color: var(--muted);
  }
  .rule-row.dragging {
    opacity: 0.5;
  }
  .grip {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--muted);
    cursor: grab;
  }
  .grip:active {
    cursor: grabbing;
  }
  .rule-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12.5px;
    color: var(--text);
  }
  .rule-toggle {
    display: flex;
    justify-content: center;
  }
  .rule-detail {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 4px 18px 16px 40px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    background: color-mix(in srgb, var(--bg) 55%, transparent);
  }
  .rule-detail p {
    margin: 0;
    color: var(--muted);
    font-size: 0.8rem;
    line-height: 1.5;
  }
  .rule-detail-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
  .rule-detail-actions .ghost {
    height: 28px;
    padding: 0 10px;
    font-size: 0.78rem;
  }
  .empty {
    margin: 0;
    padding: 22px 14px;
    color: var(--muted);
    font-size: 0.82rem;
    text-align: center;
  }
  .rule-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    justify-content: flex-end;
    flex: none;
  }
  .rule-actions .icon {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--muted);
  }
  .rule-actions .icon:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text);
  }
  .rule-actions .icon:disabled {
    opacity: 0.35;
  }
  .icon {
    padding: 4px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .danger {
    color: var(--danger);
  }
  .dialog-backdrop {
    position: fixed;
    z-index: 80;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 28px;
    background: color-mix(in srgb, #000 38%, transparent);
  }
  .rule-dialog {
    display: flex;
    flex-direction: column;
    gap: 22px;
    width: min(780px, 100%);
    max-height: min(90vh, 880px);
    overflow: auto;
    padding: 28px 30px;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: var(--bg-raised);
    box-shadow: 0 22px 64px color-mix(in srgb, #000 42%, transparent);
  }
  .dialog-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  .dialog-head .ghost.icon {
    width: 34px;
    height: 34px !important;
    min-height: 34px !important;
    padding: 0 !important;
  }
  .dialog-head strong {
    display: block;
    color: var(--text-strong);
    font-size: 1.02rem;
  }
  .dialog-head span {
    color: var(--muted);
    font-size: 0.78rem;
  }
  .step {
    display: grid;
    grid-template-columns: 26px minmax(0, 1fr);
    gap: 16px;
    padding-bottom: 22px;
    border-bottom: 1px solid var(--border);
  }
  .step.exceptions {
    border-bottom: 0;
    padding-bottom: 4px;
  }
  .bullet {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--bg-hover);
    color: var(--text);
    font-size: 0.72rem;
    font-weight: 600;
  }
  .step-main {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .step-title {
    margin: 0;
    color: var(--text-strong);
    font-size: 0.9rem;
    font-weight: 600;
  }
  .rule-dialog .step-input {
    width: 100%;
    height: 38px !important;
    min-height: 38px !important;
    padding: 0 12px !important;
    font-size: 0.86rem;
  }
  /* Every control in the dialog lines up on the same 38px baseline. */
  .rule-dialog :global(.themed-select .trigger) {
    height: 38px;
    font-size: 0.82rem;
  }
  .rule-dialog .remove {
    width: 38px;
    height: 38px !important;
    min-height: 38px !important;
    margin: 0;
    padding: 0 !important;
    flex: 0 0 auto;
  }
  .step-input.invalid {
    border-color: var(--danger);
  }
  .step-error {
    margin: 0;
    color: var(--danger);
    font-size: 0.78rem;
  }
  .rule-line {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  .type-select {
    flex: 0 0 230px;
    min-width: 0;
  }
  .value-select {
    flex: 1;
    min-width: 160px;
  }
  .rule-dialog .inline-input {
    flex: 1;
    min-width: 140px;
    height: 38px !important;
    min-height: 38px !important;
    padding: 0 10px !important;
    font-size: 0.82rem;
  }
  .link {
    align-self: flex-start;
    padding: 2px 0;
    color: var(--accent);
    font-size: 0.8rem;
    font-weight: 600;
  }
  .link:hover {
    text-decoration: underline;
  }
  .check-row {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text);
    font-size: 0.86rem;
  }
  .check-row input {
    width: 16px;
    height: 16px;
    accent-color: var(--accent);
  }
  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding-top: 6px;
    border-top: 1px solid var(--border);
  }
  .dialog-actions .primary,
  .dialog-actions .ghost {
    height: 38px !important;
    min-height: 38px !important;
    padding: 0 18px !important;
    font-size: 0.84rem;
  }
  .primary,
  .ghost {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
</style>
