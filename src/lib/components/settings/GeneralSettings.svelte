<script lang="ts">
  import { onMount } from "svelte";
  import { CircleAlert, Plus, X } from "lucide-svelte";
  import { api } from "../../api";
  import { loadPreferences, updatePreferences, type SettingsPreferences } from "../../settings";
  import {
    activeSpaceId,
    defaultSpaceGeneral,
    loadSpaceGeneral,
    saveSpaceGeneral
  } from "../../spaces";
  import { app } from "../../stores/app.svelte";
  import { appearance } from "../../stores/appearance.svelte";
  import type { TimeFormat } from "../../format";
  import type {
    DesktopCloseAction,
    DesktopMinimizeAction,
    DesktopPreferences,
    MessageOrder,
    SpaceGeneral
  } from "../../types";
  import ToggleSwitch from "../ToggleSwitch.svelte";
  import ThemedSelect from "../ThemedSelect.svelte";
  import { errorMessage } from "./errors";

  let preferences = $state<SettingsPreferences>(loadPreferences());
  let preferenceError = $state<string | null>(null);
  let trustedInput = $state("");
  let editingSpaceId = $state<number>(activeSpaceId() ?? app.activeSpaceId ?? 1);
  let desktopPreferences = $state<DesktopPreferences>(
    desktopFromGeneral(defaultSpaceGeneral())
  );

  const editingActiveSpace = $derived(
    editingSpaceId === (app.activeSpaceId ?? activeSpaceId())
  );

  function desktopFromGeneral(general: SpaceGeneral): DesktopPreferences {
    return {
      notifications: general.notifications,
      newMailSound: general.new_mail_sound,
      sentMailSound: general.sent_mail_sound,
      closeAction: general.close_action,
      minimizeAction: general.minimize_action
    };
  }

  function generalFromPreferences(): SpaceGeneral {
    return {
      time_format: preferences.time_format,
      message_order: preferences.message_order,
      mark_read_delay: preferences.mark_read_delay,
      attachment_thumbnails: preferences.attachment_thumbnails,
      reveal_download: preferences.reveal_download,
      always_load_images: preferences.always_load_images,
      profile_pictures: preferences.profile_pictures,
      trusted_senders: preferences.trusted_senders,
      notifications: desktopPreferences.notifications,
      new_mail_sound: desktopPreferences.newMailSound,
      sent_mail_sound: desktopPreferences.sentMailSound,
      close_action: desktopPreferences.closeAction,
      minimize_action: desktopPreferences.minimizeAction
    };
  }

  function sharedPreferences(): Partial<SettingsPreferences> {
    return {
      time_format: preferences.time_format,
      message_order: preferences.message_order,
      mark_read_delay: preferences.mark_read_delay,
      attachment_thumbnails: preferences.attachment_thumbnails,
      reveal_download: preferences.reveal_download,
      always_load_images: preferences.always_load_images,
      profile_pictures: preferences.profile_pictures,
      trusted_senders: preferences.trusted_senders
    };
  }

  function persistGeneral(patch: Partial<SpaceGeneral>): void {
    preferences = { ...preferences, ...patch };
    saveSpaceGeneral(editingSpaceId, generalFromPreferences());
    if (editingActiveSpace) {
      preferences = updatePreferences(sharedPreferences());
    }
  }

  function selectEditingSpace(id: number): void {
    editingSpaceId = id;
    const general = loadSpaceGeneral(id);
    preferences = { ...preferences, ...general };
    desktopPreferences = desktopFromGeneral(general);
    preferenceError = null;
    void loadDesktopPreferences(id);
  }

  async function loadDesktopPreferences(spaceId = editingSpaceId): Promise<void> {
    try {
      const loaded = await api.getDesktopPreferences(spaceId);
      if (spaceId === editingSpaceId) desktopPreferences = loaded;
    } catch (error) {
      if (spaceId === editingSpaceId) preferenceError = errorMessage(error);
    }
  }

  function setClockFormat(format: TimeFormat) {
    persistGeneral({ time_format: format });
    if (editingActiveSpace) appearance.setTimeFormat(format);
  }

  function setMarkReadDelay(value: string) {
    persistGeneral({ mark_read_delay: Number(value) });
  }

  function setMessageOrder(value: string) {
    const order: MessageOrder = value === "oldest" ? "oldest" : "newest";
    persistGeneral({ message_order: order });
    if (editingActiveSpace) {
      window.dispatchEvent(new CustomEvent("auramail:message-order-changed"));
    }
  }

  function setAttachmentThumbnails(value: boolean) {
    persistGeneral({ attachment_thumbnails: value });
    if (editingActiveSpace) {
      window.dispatchEvent(new CustomEvent("auramail:preferences-changed"));
    }
  }

  function setRevealDownload(value: boolean) {
    persistGeneral({ reveal_download: value });
    if (editingActiveSpace) {
      window.dispatchEvent(new CustomEvent("auramail:preferences-changed"));
    }
  }

  function setProfilePictures(value: boolean) {
    persistGeneral({ profile_pictures: value });
    notifyPreferences();
  }

  function setAlwaysLoadImages(value: boolean) {
    persistGeneral({ always_load_images: value });
    notifyPreferences();
  }

  onMount(() => {
    void loadDesktopPreferences();
  });

  async function updateDesktop(patch: Partial<DesktopPreferences>) {
    const previous = desktopPreferences;
    const next = { ...desktopPreferences, ...patch };
    desktopPreferences = next;
    try {
      const saved = await api.setDesktopPreferences(next, editingSpaceId);
      desktopPreferences = saved;
      saveSpaceGeneral(editingSpaceId, generalFromPreferences());
      preferenceError = null;
    } catch (error) {
      desktopPreferences = previous;
      preferenceError = errorMessage(error);
    }
  }

  function notifyPreferences() {
    persistGeneral({});
    void api
      .setMailPreferences(
        {
          trusted_senders: preferences.trusted_senders,
          always_load_images: preferences.always_load_images,
          profile_pictures: preferences.profile_pictures
        },
        editingSpaceId
      )
      .catch(() => {});
    if (editingActiveSpace) {
      window.dispatchEvent(new CustomEvent("auramail:preferences-changed"));
    }
  }

  function addTrustedSender() {
    const sender = trustedInput.trim().toLowerCase();
    if (!sender || !sender.includes("@")) return;
    preferences = {
      ...preferences,
      trusted_senders: Array.from(new Set([...preferences.trusted_senders, sender]))
    };
    trustedInput = "";
    notifyPreferences();
  }

  function removeTrustedSender(sender: string) {
    preferences = {
      ...preferences,
      trusted_senders: preferences.trusted_senders.filter((item) => item !== sender)
    };
    notifyPreferences();
  }
</script>

<div class="settings-inner">
  <div class="edit-space-row">
    <span>Edit space</span>
    <ThemedSelect
      value={editingSpaceId}
      options={app.spaces.map((space) => ({
        value: String(space.id),
        label: space.name
      }))}
      ariaLabel="Edit space"
      onchange={(value) => selectEditingSpace(Number(value))}
    />
  </div>

  {#if preferenceError}<p class="alert" role="alert"><CircleAlert size={14} />{preferenceError}</p>{/if}

  <div class="settings-label">Notifications</div>
  <section class="settings-panel">
    <div class="settings-row">
      <strong>System notifications</strong>
      <ToggleSwitch
        checked={desktopPreferences.notifications}
        label="System notifications"
        onchange={(value) => void updateDesktop({ notifications: value })}
      />
    </div>
    <div class="settings-row">
      <strong>New mail sound</strong>
      <ToggleSwitch
        checked={desktopPreferences.newMailSound}
        label="New mail sound"
        onchange={(value) => void updateDesktop({ newMailSound: value })}
      />
    </div>
    <div class="settings-row">
      <strong>Sent mail sound</strong>
      <ToggleSwitch
        checked={desktopPreferences.sentMailSound}
        label="Sent mail sound"
        onchange={(value) => void updateDesktop({ sentMailSound: value })}
      />
    </div>
  </section>

  <div class="settings-label">System tray</div>
  <section class="settings-panel">
    <div class="settings-row">
      <strong>Close window</strong>
      <ThemedSelect
        value={desktopPreferences.closeAction}
        options={[
          { value: "tray", label: "Hide to tray" },
          { value: "quit", label: "Quit AuraMail" }
        ]}
        ariaLabel="Close window behavior"
        onchange={(value) =>
          void updateDesktop({ closeAction: value as DesktopCloseAction })}
      />
    </div>
    <div class="settings-row">
      <strong>Minimize window</strong>
      <ThemedSelect
        value={desktopPreferences.minimizeAction}
        options={[
          { value: "taskbar", label: "Keep in taskbar" },
          { value: "tray", label: "Hide to tray" }
        ]}
        ariaLabel="Minimize window behavior"
        onchange={(value) =>
          void updateDesktop({
            minimizeAction: value as DesktopMinimizeAction
          })}
      />
    </div>
  </section>

  <div class="settings-label">Reading</div>
  <section class="settings-panel">
    <div class="settings-row">
      <strong>Clock</strong>
      <ThemedSelect
        value={preferences.time_format}
        options={[
          { value: "system", label: "System default" },
          { value: "12h", label: "12-hour" },
          { value: "24h", label: "24-hour" }
        ]}
        ariaLabel="Clock"
        onchange={(value) => setClockFormat(value as TimeFormat)}
      />
    </div>
    <div class="settings-row">
      <strong>Conversation order</strong>
      <ThemedSelect
        value={preferences.message_order}
        options={[
          { value: "newest", label: "Newest first" },
          { value: "oldest", label: "Oldest first" }
        ]}
        ariaLabel="Conversation order"
        onchange={setMessageOrder}
      />
    </div>
    <div class="settings-row">
      <strong>Mark as read</strong>
      <ThemedSelect
        value={String(preferences.mark_read_delay)}
        options={[
          { value: "0", label: "Immediately" },
          { value: "2", label: "After 2 seconds" },
          { value: "5", label: "After 5 seconds" },
          { value: "10", label: "After 10 seconds" },
          { value: "30", label: "After 30 seconds" },
          { value: "-1", label: "Never" }
        ]}
        ariaLabel="Mark as read"
        onchange={setMarkReadDelay}
      />
    </div>
  </section>

  <div class="settings-label">Attachments</div>
  <section class="settings-panel">
    <div class="settings-row">
      <strong>Show image thumbnails</strong>
      <ToggleSwitch
        checked={preferences.attachment_thumbnails}
        label="Show image thumbnails"
        onchange={setAttachmentThumbnails}
      />
    </div>
    <div class="settings-row">
      <strong>Open folder after saving</strong>
      <ToggleSwitch
        checked={preferences.reveal_download}
        label="Open folder after saving"
        onchange={setRevealDownload}
      />
    </div>
  </section>

  <div class="settings-label">Privacy</div>
  <div class="settings-bento">
    <section class="settings-panel">
      <div class="settings-row">
        <strong>Always load remote images</strong>
        <ToggleSwitch
          checked={preferences.always_load_images}
          label="Always load remote images"
          onchange={setAlwaysLoadImages}
        />
      </div>
      <div class="settings-row">
        <strong>Sender profile pictures</strong>
        <ToggleSwitch
          checked={preferences.profile_pictures}
          label="Sender profile pictures"
          onchange={setProfilePictures}
        />
      </div>
    </section>

    <section class="settings-panel">
      <div class="trusted-title">Trusted senders</div>
      <div class="inline-field trusted-entry">
        <input
          bind:value={trustedInput}
          placeholder="newsletter@example.com"
          aria-label="Trusted sender email"
          onkeydown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              addTrustedSender();
            }
          }}
        />
        <button class="ghost" type="button" onclick={addTrustedSender}>
          <Plus size={14} />
          Add
        </button>
      </div>
      {#if preferences.trusted_senders.length > 0}
        <ul class="trusted-list" aria-label="Trusted senders">
          {#each preferences.trusted_senders as sender (sender)}
            <li>
              <span>{sender}</span>
              <button
                class="icon-btn"
                type="button"
                aria-label={`Remove ${sender} from trusted senders`}
                onclick={() => removeTrustedSender(sender)}
              >
                <X size={14} />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  </div>
</div>

<style>
  .trusted-title {
    margin-bottom: 8px;
    color: var(--text);
    font-size: 11.5px;
    font-weight: 620;
  }
  .trusted-entry input {
    width: 100%;
    flex: 1;
    text-align: left;
  }
  .trusted-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 10px 0 8px;
    padding: 0;
    list-style: none;
  }
  .trusted-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-height: 32px;
    padding: 3px 5px 3px 8px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--bg) 45%, transparent);
    color: var(--text);
    font-size: 10px;
  }
  .icon-btn {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    flex: none;
    padding: 0;
    border-radius: 6px;
    color: var(--muted);
    line-height: 1;
  }
  .icon-btn:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .edit-space-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 12px;
    color: var(--muted);
    font-size: 10.5px;
  }
  .edit-space-row :global(.themed-select) {
    width: 170px;
  }
  :global(.settings-row .themed-select) {
    width: 180px;
    flex: none;
  }
</style>
