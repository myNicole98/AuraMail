<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import {
    Check,
    ChevronDown,
    CircleAlert,
    Eye,
    EyeOff,
    KeyRound,
    LoaderCircle,
    RefreshCw,
    Trash2
  } from "lucide-svelte";
  import {
    modelIconUrl,
    providerById,
    providerForUrl,
    providerIconUrls,
    providers,
    type AiProvider,
    type AiProviderId
  } from "../../ai/providers";
  import { api } from "../../api";
  import { activeSpaceId } from "../../spaces";
  import { app } from "../../stores/app.svelte";
  import {
    loadPreferences,
    savePreferences,
    validateAiSettings,
    type SettingsPreferences
  } from "../../settings";
  import type {
    AiConfig,
    AiConfigInput,
    AiVoiceProfile,
    DiscoveredProvider,
    SemanticStatus
  } from "../../types";
  import ToggleSwitch from "../ToggleSwitch.svelte";
  import ThemedSelect from "../ThemedSelect.svelte";
  import { errorMessage } from "./errors";

  let preferences = $state<SettingsPreferences>(loadPreferences());
  let editingSpaceId = $state<number>(activeSpaceId() ?? app.activeSpaceId ?? 1);
  let ai = $state<AiConfig | null>(null);
  let aiBaseUrl = $state("");
  let aiModel = $state("");
  let aiEmbeddingBaseUrl = $state("");
  let aiEmbeddingModel = $state("");
  let aiEmbeddingKey = $state("");
  let embeddingProvider = $state<AiProviderId>("custom");
  let embeddingHasKey = $state(false);
  let aiEmbeddingModels = $state<string[]>([]);
  let embeddingModelsLoading = $state(false);
  let embeddingModelsError = $state<string | null>(null);
  let embeddingModelRequest = 0;
  let aiKey = $state("");
  let aiProvider = $state<AiProviderId>("openai");
  let aiDiscovered = $state<DiscoveredProvider[]>([]);
  let aiScanning = $state(false);
  let aiModels = $state<string[]>([]);
  let aiModelsLoading = $state(false);
  let aiModelsError = $state<string | null>(null);
  let modelRequest = 0;
  let providerMenuOpen = $state(false);
  let modelMenuOpen = $state(false);
  let embeddingProviderMenuOpen = $state(false);
  let embeddingModelMenuOpen = $state(false);
  let aiErrors = $state<Record<string, string>>({});
  let aiEnabled = $state(false);
  let aiLoading = $state(true);
  let aiSaving = $state(false);
  let aiTesting = $state(false);
  let aiNotice = $state<string | null>(null);
  let aiError = $state<string | null>(null);
  let showKey = $state(false);
  let showEmbeddingKey = $state(false);
  let voiceProfile = $state<AiVoiceProfile | null>(null);
  let voiceLoading = $state(false);
  let voiceLearning = $state(false);
  let voiceClearing = $state(false);
  let voiceError = $state<string | null>(null);
  let voiceNotice = $state<string | null>(null);
  let semanticStatus = $state<SemanticStatus | null>(null);
  let semanticError = $state<string | null>(null);
  let semanticTimer: ReturnType<typeof setTimeout> | null = null;

  let aiConfigured = $derived(ai?.configured ?? false);
  let providerHasKey = $state(false);
  let aiProviderInfo = $derived(providerById(aiProvider));
  let aiModelIcon = $derived(modelIconUrl(aiModel));
  let detectedProviders = $derived(
    providers.filter((provider) =>
      aiDiscovered.some((candidate) => candidate.id === provider.id)
    )
  );
  let otherProviders = $derived(
    providers.filter(
      (provider) =>
        !aiDiscovered.some((candidate) => candidate.id === provider.id)
    )
  );
  let embeddingProviderInfo = $derived(providerById(embeddingProvider));
  let embeddingModelIcon = $derived(modelIconUrl(aiEmbeddingModel));
  let embeddingShowEndpoint = $derived(!embeddingProviderInfo.fixed);
  let embeddingUsesChatKey = $derived(
    aiEmbeddingBaseUrl.trim().replace(/\/+$/, "") !== "" &&
      aiBaseUrl.trim().replace(/\/+$/, "") ===
        aiEmbeddingBaseUrl.trim().replace(/\/+$/, "")
  );
  let embeddingShowKey = $derived(
    !embeddingUsesChatKey &&
      embeddingProvider !== "ollama" &&
      embeddingProvider !== "lmstudio"
  );
  let embeddingModels = $derived(
    Array.from(
      new Set([
        ...(aiDiscovered
          .find((local) => local.id === embeddingProvider)
          ?.models.filter((model) =>
            /embed|bge|e5|nomic|mxbai|minilm/i.test(model)
          ) ?? []),
        ...aiEmbeddingModels,
        ...(aiEmbeddingModel ? [aiEmbeddingModel] : [])
      ])
    )
  );
  let aiShowEndpoint = $derived(!aiProviderInfo.fixed);
  let aiShowKey = $derived(aiProviderInfo.requiresKey || aiProvider === "custom");
  let aiKeyReady = $derived(
    !aiProviderInfo.requiresKey || providerHasKey || aiKey.trim().length > 0
  );
  let aiDirty = $derived(
    ai !== null &&
      (aiBaseUrl.trim() !== ai.base_url ||
        aiModel.trim() !== ai.model ||
        aiEmbeddingBaseUrl.trim() !== ai.embedding_base_url ||
        aiEmbeddingModel.trim() !== ai.embedding_model ||
        aiKey.length > 0 ||
        aiEmbeddingKey.length > 0)
  );

  function applyAi(config: AiConfig) {
    ai = config;
    aiEnabled = config.enabled;
    aiBaseUrl = config.base_url;
    aiModel = config.model;
    aiEmbeddingBaseUrl = config.embedding_base_url;
    aiEmbeddingModel = config.embedding_model;
    aiEmbeddingKey = "";
    aiEmbeddingModels = [];
    embeddingHasKey = config.has_embedding_key;
    embeddingProvider = config.embedding_base_url.trim()
      ? providerForUrl(config.embedding_base_url)
      : "custom";
    aiKey = "";
    aiErrors = {};
    providerHasKey = config.has_key;
    aiProvider = providerForUrl(config.base_url);
    if (!config.base_url) {
      const provider = providerById(aiProvider);
      aiBaseUrl = provider.baseUrl;
      aiModel = "";
    }
  }

  async function loadModels(): Promise<void> {
    const request = ++modelRequest;
    if (!aiBaseUrl.trim()) {
      aiModels = [];
      aiModelsError = null;
      aiModelsLoading = false;
      return;
    }
    aiModelsLoading = true;
    aiModelsError = null;
    try {
      const models = await api.listAiModels(aiBaseUrl.trim(), aiKey.trim() || undefined, editingSpaceId);
      if (request !== modelRequest) return;
      aiModels = models;
      if (models.length > 0 && !models.includes(aiModel)) {
        aiModel = models[0];
      }
    } catch (error) {
      if (request !== modelRequest) return;
      aiModels = [];
      aiModelsError = errorMessage(error);
    } finally {
      if (request === modelRequest) aiModelsLoading = false;
    }
  }

  async function scanLocalProviders() {
    if (aiScanning) return;
    aiScanning = true;
    try {
      aiDiscovered = await api.discoverAiProviders().catch(() => []);
    } finally {
      aiScanning = false;
    }
  }

  async function loadEmbeddingModels(): Promise<void> {
    const request = ++embeddingModelRequest;
    const base = aiEmbeddingBaseUrl.trim();
    if (!base) {
      aiEmbeddingModels = [];
      embeddingModelsError = null;
      embeddingModelsLoading = false;
      return;
    }
    embeddingModelsLoading = true;
    embeddingModelsError = null;
    try {
      const models = await api.listAiModels(
        base,
        aiEmbeddingKey.trim() || undefined,
        editingSpaceId
      );
      if (request !== embeddingModelRequest) return;
      aiEmbeddingModels = models.filter((model) =>
        /embed|bge|e5|nomic|mxbai|minilm/i.test(model)
      );
    } catch (error) {
      if (request !== embeddingModelRequest) return;
      aiEmbeddingModels = [];
      embeddingModelsError = errorMessage(error);
    } finally {
      if (request === embeddingModelRequest) embeddingModelsLoading = false;
    }
  }

  async function selectEmbeddingProvider(id: AiProviderId): Promise<void> {
    embeddingProvider = id;
    embeddingProviderMenuOpen = false;
    embeddingModelMenuOpen = false;
    embeddingModelsError = null;
    aiEmbeddingModels = [];
    const provider = providerById(id);
    aiEmbeddingModel = "";
    aiEmbeddingKey = "";
    embeddingHasKey = false;
    if (id === "custom") {
      aiEmbeddingBaseUrl = "";
      return;
    }
    aiEmbeddingBaseUrl = provider.baseUrl;
    embeddingHasKey = await api
      .aiHasKey(aiEmbeddingBaseUrl, editingSpaceId)
      .catch(() => false);
    await loadEmbeddingModels();
    const detected =
      aiDiscovered
        .find((local) => local.id === id)
        ?.models.find((model) =>
          /embed|bge|e5|nomic|mxbai|minilm/i.test(model)
        ) ?? "";
    const suggested = aiEmbeddingModels[0] ?? detected;
    if (suggested) {
      aiEmbeddingModel = suggested;
      await saveAi();
    }
  }

  async function loadAi() {
    aiLoading = true;
    aiModels = [];
    aiModelsError = null;
    modelMenuOpen = false;
    try {
      const config = await api.getAiConfig(editingSpaceId);
      applyAi(config);
      aiError = null;
      aiDiscovered = await api.discoverAiProviders().catch(() => []);
      await loadEmbeddingModels();
      if (
        config.base_url &&
        (providerHasKey || !providerById(aiProvider).requiresKey)
      ) {
        const before = aiModel;
        await loadModels();
        if (aiModel && aiModel !== before) await saveAi();
      }
    } catch (error) {
      aiError = errorMessage(error);
    } finally {
      aiLoading = false;
    }
  }

  async function loadAiPreferences() {
    try {
      const stored = await api.getAiPreferences(editingSpaceId);
      preferences = { ...preferences, ...stored };
      savePreferences(preferences);
    } catch {
      return;
    }
  }

  async function saveAiPreferences() {
    try {
      const stored = await api.setAiPreferences({
        auto_classify: preferences.auto_classify,
        auto_rail: preferences.auto_rail,
        full_body_context: preferences.full_body_context,
        match_voice: preferences.match_voice
      }, editingSpaceId);
      preferences = { ...preferences, ...stored };
      savePreferences(preferences);
      aiError = null;
    } catch (error) {
      aiError = errorMessage(error);
    }
  }

  async function loadVoiceProfile() {
    voiceLoading = true;
    try {
      voiceProfile = await api.getAiVoiceProfile(editingSpaceId);
      voiceError = null;
    } catch (error) {
      voiceError = errorMessage(error);
    } finally {
      voiceLoading = false;
    }
  }

  async function learnVoice() {
    if (voiceLearning || voiceClearing) return;
    voiceLearning = true;
    voiceError = null;
    voiceNotice = null;
    try {
      voiceProfile = await api.learnAiVoice(editingSpaceId);
      voiceNotice = "Style updated.";
    } catch (error) {
      voiceError = errorMessage(error);
    } finally {
      voiceLearning = false;
    }
  }

  async function clearVoice() {
    if (voiceLearning || voiceClearing) return;
    voiceClearing = true;
    voiceError = null;
    voiceNotice = null;
    try {
      await api.clearAiVoiceProfile(editingSpaceId);
      voiceProfile = null;
      if (preferences.match_voice) {
        preferences = { ...preferences, match_voice: false };
        await saveAiPreferences();
      }
      voiceNotice = "Style cleared. Matching is off.";
    } catch (error) {
      voiceError = errorMessage(error);
    } finally {
      voiceClearing = false;
    }
  }

  function setMatchVoice(value: boolean) {
    preferences = { ...preferences, match_voice: value };
    void (async () => {
      await saveAiPreferences();
      if (value && !voiceProfile) void learnVoice();
    })();
  }

  async function refreshSemanticStatus(startIfPending = true): Promise<void> {
    if (semanticTimer) {
      clearTimeout(semanticTimer);
      semanticTimer = null;
    }
    try {
      let status = await api.getSemanticStatus(editingSpaceId);
      if (startIfPending && status.configured && status.pending > 0 && !status.indexing) {
        status = await api.startSemanticIndex(editingSpaceId);
      }
      semanticStatus = status;
      semanticError = status.error;
      if (status.indexing) {
        semanticTimer = setTimeout(() => {
          semanticTimer = null;
          void refreshSemanticStatus(false);
        }, 1200);
      }
    } catch (error) {
      semanticError = errorMessage(error);
    }
  }

  function setAutoClassify(value: boolean) {
    preferences = { ...preferences, auto_classify: value };
    void saveAiPreferences();
  }

  function setAutoRail(value: boolean) {
    preferences = { ...preferences, auto_rail: value };
    void saveAiPreferences();
  }

  function setFullBodyContext(value: boolean) {
    preferences = { ...preferences, full_body_context: value };
    void saveAiPreferences();
  }

  async function saveAi(embeddingKeyOverride?: string | null): Promise<boolean> {
    const errors = validateAiSettings(aiBaseUrl, aiModel);
    aiErrors = errors;
    aiNotice = null;
    aiError = null;
    if (Object.keys(errors).length > 0) return false;
    aiSaving = true;
    try {
      const input: AiConfigInput = {
        base_url: aiBaseUrl.trim(),
        model: aiModel.trim(),
        embedding_base_url: aiEmbeddingBaseUrl.trim(),
        embedding_model: aiEmbeddingModel.trim(),
        extra_headers: []
      };
      if (aiKey.trim()) input.api_key = aiKey.trim();
      if (embeddingKeyOverride !== undefined) {
        input.embedding_api_key = embeddingKeyOverride;
      } else if (aiEmbeddingKey.trim()) {
        input.embedding_api_key = aiEmbeddingKey.trim();
      }
      applyAi(await api.setAiConfig(input, editingSpaceId));
      await refreshSemanticStatus(true);
      return true;
    } catch (error) {
      aiError = errorMessage(error);
      return false;
    } finally {
      aiSaving = false;
    }
  }

  async function selectProvider(id: AiProviderId): Promise<void> {
    providerMenuOpen = false;
    modelMenuOpen = false;
    const provider = providerById(id);
    aiProvider = id;
    aiErrors = {};
    aiNotice = null;
    aiError = null;
    modelRequest += 1;
    aiModels = [];
    aiModelsError = null;
    if (id === "custom") {
      providerHasKey = false;
      return;
    }
    aiBaseUrl = provider.baseUrl;
    aiModel = "";
    providerHasKey = await api.aiHasKey(provider.baseUrl, editingSpaceId).catch(() => false);
    const local = aiDiscovered.find((candidate) => candidate.id === id);
    if (local?.models.length) {
      aiModels = local.models;
      aiModel = local.models[0];
    } else if (!provider.requiresKey || providerHasKey || aiKey.trim()) {
      await loadModels();
    }
    if (aiModel.trim()) await saveAi();
  }

  async function refreshModelsAndSave(): Promise<void> {
    await loadModels();
    if (aiModel.trim()) await saveAi();
  }

  async function testAi() {
    aiNotice = null;
    aiError = null;
    if (aiDirty) {
      const saved = await saveAi();
      if (!saved) return;
    }
    aiTesting = true;
    try {
      await api.testAi(editingSpaceId);
      aiNotice = "Connection succeeded.";
    } catch (error) {
      aiError = errorMessage(error);
    } finally {
      aiTesting = false;
    }
  }

  async function clearAiKey() {
    if (!window.confirm("Remove the stored AI API key?")) return;
    const errors = validateAiSettings(aiBaseUrl, aiModel);
    aiErrors = errors;
    if (Object.keys(errors).length > 0) return;
    aiSaving = true;
    aiNotice = null;
    aiError = null;
    try {
      applyAi(
        await api.setAiConfig({
          base_url: aiBaseUrl.trim(),
          model: aiModel.trim(),
          embedding_base_url: aiEmbeddingBaseUrl.trim(),
          embedding_model: aiEmbeddingModel.trim(),
          extra_headers: [],
          api_key: ""
        }, editingSpaceId)
      );
      aiNotice = "Stored API key removed.";
    } catch (error) {
      aiError = errorMessage(error);
    } finally {
      aiSaving = false;
    }
  }


  async function setAiEnabled(value: boolean): Promise<void> {
    const previous = aiEnabled;
    aiEnabled = value;
    aiNotice = null;
    aiError = null;
    try {
      applyAi(await api.setAiEnabled(value, editingSpaceId));
      if (editingSpaceId === app.activeSpaceId) {
        app.aiConfigured = await api.aiStatus(editingSpaceId);
      }
    } catch (error) {
      aiEnabled = previous;
      aiError = errorMessage(error);
    }
  }

  function selectEditingSpace(id: number): void {
    if (id === editingSpaceId) return;
    editingSpaceId = id;
    modelRequest += 1;
    embeddingModelRequest += 1;
    ai = null;
    aiLoading = true;
    aiError = null;
    aiNotice = null;
    voiceProfile = null;
    semanticStatus = null;
    semanticError = null;
    void initializeAiSettings();
  }

  async function initializeAiSettings() {
    await Promise.all([loadAi(), loadAiPreferences(), loadVoiceProfile()]);
    if (aiConfigured && preferences.match_voice && !voiceProfile) {
      void learnVoice();
    }
    await refreshSemanticStatus(true);
  }

  onMount(() => {
    void initializeAiSettings();
  });

  onDestroy(() => {
    if (semanticTimer) clearTimeout(semanticTimer);
  });
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

  {#if aiNotice}
    <p class="notice" role="status"><Check size={14} />{aiNotice}</p>
  {/if}
  {#if aiError}
    <p class="alert" role="alert"><CircleAlert size={14} />{aiError}</p>
  {/if}

  {#if aiLoading}
    <p class="empty loading-line" role="status">
      <span class="ai-spinner"><LoaderCircle size={14} /></span>
      Loading AI settings…
    </p>
  {:else}
    <div class="ai-enable-row">
      <strong>Enable Aura AI</strong>
      <ToggleSwitch
        checked={aiEnabled}
        label="Enable Aura AI"
        onchange={(value) => void setAiEnabled(value)}
      />
    </div>

    {#if aiEnabled}
    {@const selectedIcons = providerIconUrls(aiProvider)}
    {#snippet providerOption(provider: AiProvider)}
      {@const icons = providerIconUrls(provider.id)}
      {@const selected = aiProvider === provider.id}
      {@const detected = aiDiscovered.some(
        (candidate) => candidate.id === provider.id
      )}
      <button
        type="button"
        class:active={selected}
        role="option"
        aria-selected={selected}
        onclick={() => selectProvider(provider.id)}
      >
        <span class="provider-icon-wrap" class:has-dark={Boolean(icons.dark)}>
          {#if icons.light}<img class="provider-icon icon-light" src={icons.light} alt="" />{/if}
          {#if icons.dark}<img class="provider-icon icon-dark" src={icons.dark} alt="" />{/if}
        </span>
        <span class="provider-label">{provider.label}</span>
        <span class="menu-trailing">
          {#if detected}
            <span class="detected" title="Detected on this computer"></span>
          {/if}
          {#if selected}<Check size={14} />{/if}
        </span>
      </button>
    {/snippet}
    {#snippet embeddingProviderOption(provider: AiProvider)}
      {@const icons = providerIconUrls(provider.id)}
      {@const selected = embeddingProvider === provider.id}
      {@const detected = aiDiscovered.some(
        (candidate) => candidate.id === provider.id
      )}
      <button
        type="button"
        class:active={selected}
        role="option"
        aria-selected={selected}
        onclick={() => void selectEmbeddingProvider(provider.id)}
      >
        <span class="provider-icon-wrap" class:has-dark={Boolean(icons.dark)}>
          {#if icons.light}<img class="provider-icon icon-light" src={icons.light} alt="" />{/if}
          {#if icons.dark}<img class="provider-icon icon-dark" src={icons.dark} alt="" />{/if}
        </span>
        <span class="provider-label">{provider.label}</span>
        <span class="menu-trailing">
          {#if detected}
            <span class="detected" title="Detected on this computer"></span>
          {/if}
          {#if selected}<Check size={14} />{/if}
        </span>
      </button>
    {/snippet}
    <div class="settings-label">
      <span>Provider &amp; connection</span>
      <span class="status-pill" class:on={aiConfigured}>
        <KeyRound size={13} />
        {aiConfigured ? "Configured" : "Not configured"}
      </span>
    </div>
    <div class="ai-grid">
      <section class="settings-panel ai-panel">
        <div class="panel-head">
          <h4>Provider</h4>
          <button
            class="icon-btn scan-btn"
            class:scanning={aiScanning}
            type="button"
            aria-label="Scan for local providers"
            title="Scan for local providers"
            disabled={aiScanning}
            onclick={() => void scanLocalProviders()}
          >
            <RefreshCw size={14} />
          </button>
        </div>
        <div class="provider-select">
          <button
            class="provider-trigger"
            class:open={providerMenuOpen}
            type="button"
            aria-haspopup="listbox"
            aria-expanded={providerMenuOpen}
            onclick={() => {
              modelMenuOpen = false;
              providerMenuOpen = !providerMenuOpen;
            }}
          >
            <span class="provider-icon-wrap" class:has-dark={Boolean(selectedIcons.dark)}>
              {#if selectedIcons.light}<img class="provider-icon icon-light" src={selectedIcons.light} alt="" />{/if}
              {#if selectedIcons.dark}<img class="provider-icon icon-dark" src={selectedIcons.dark} alt="" />{/if}
            </span>
            <span class="provider-label">{aiProviderInfo.label}</span>
            <ChevronDown size={14} />
          </button>
          {#if providerMenuOpen}
            <button
              class="provider-backdrop"
              type="button"
              aria-label="Close provider menu"
              onclick={() => (providerMenuOpen = false)}
            ></button>
            <div
              class="provider-menu"
              role="listbox"
              aria-label="AI provider"
              tabindex="-1"
              onkeydown={(event) => {
                if (event.key === "Escape") {
                  providerMenuOpen = false;
                  event.stopPropagation();
                }
              }}
            >
              {#if detectedProviders.length > 0}
                <div class="menu-group-label">Detected locally</div>
                {#each detectedProviders as provider (provider.id)}
                  {@render providerOption(provider)}
                {/each}
                <div class="menu-separator"></div>
              {/if}
              {#if detectedProviders.length > 0}
                <div class="menu-group-label">Other providers</div>
              {/if}
              {#each otherProviders as provider (provider.id)}
                {@render providerOption(provider)}
              {/each}
            </div>
          {/if}
        </div>

        {#if aiShowEndpoint}
          <label class="field ai-field">
            <span>Base URL</span>
            <input
              bind:value={aiBaseUrl}
              placeholder="https://api.example.com/v1"
              class:invalid={Boolean(aiErrors.base_url)}
              aria-invalid={Boolean(aiErrors.base_url)}
              oninput={() => (aiErrors = { ...aiErrors, base_url: "" })}
              onchange={() => void refreshModelsAndSave()}
            />
            {#if aiErrors.base_url}<small class="field-error">{aiErrors.base_url}</small>{/if}
          </label>
        {/if}

        {#if aiShowKey}
          <label class="field ai-field">
            <span>API key</span>
            <div class="secret-row">
              <input
                type={showKey ? "text" : "password"}
                bind:value={aiKey}
                autocomplete="off"
                placeholder={providerHasKey ? "••••••••••••" : aiProviderInfo.keyPlaceholder}
                onchange={() => void refreshModelsAndSave()}
              />
              <button
                class="icon-btn"
                type="button"
                aria-label={showKey ? "Hide API key" : "Show API key"}
                title={showKey ? "Hide key" : "Show key"}
                onclick={() => (showKey = !showKey)}
              >
                {#if showKey}<EyeOff size={15} />{:else}<Eye size={15} />{/if}
              </button>
            </div>
            {#if providerHasKey}
              <small class="help key-saved"><Check size={12} /> API key saved</small>
            {/if}
          </label>
        {/if}
      </section>

      <section class="settings-panel ai-panel">
        <h4>Conversation Model</h4>
        <div class="field">
          {#if aiModels.length > 0}
            <div class="provider-select">
              <button
                class="provider-trigger"
                class:open={modelMenuOpen}
                type="button"
                aria-label="Conversation Model"
                disabled={!aiKeyReady}
                aria-haspopup="listbox"
                aria-expanded={modelMenuOpen}
                onclick={() => {
                  providerMenuOpen = false;
                  modelMenuOpen = !modelMenuOpen;
                }}
              >
                {#if aiModelIcon}<img class="model-icon" src={aiModelIcon} alt="" />{/if}
                <span class="provider-label">{aiModel || "Select model"}</span>
                <ChevronDown size={14} />
              </button>
              {#if modelMenuOpen}
                <button
                  class="provider-backdrop"
                  type="button"
                  aria-label="Close model menu"
                  onclick={() => (modelMenuOpen = false)}
                ></button>
                <div
                  class="provider-menu"
                  role="listbox"
                  aria-label="Conversation Model"
                  tabindex="-1"
                  onkeydown={(event) => {
                    if (event.key === "Escape") {
                      modelMenuOpen = false;
                      event.stopPropagation();
                    }
                  }}
                >
                  {#if aiModel && !aiModels.includes(aiModel)}
                    <button
                      type="button"
                      class="active"
                      role="option"
                      aria-selected="true"
                      onclick={() => (modelMenuOpen = false)}
                    >
                      <span class="provider-label">{aiModel}</span>
                      <Check size={14} />
                    </button>
                  {/if}
                  {#each aiModels as model (model)}
                    <button
                      type="button"
                      class:active={aiModel === model}
                      role="option"
                      aria-selected={aiModel === model}
                      onclick={() => {
                        aiModel = model;
                        modelMenuOpen = false;
                        void saveAi();
                      }}
                    >
                      <span class="provider-label">{model}</span>
                      {#if aiModel === model}<Check size={14} />{/if}
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
          {:else}
            <div class="input-with-icon">
              {#if aiModelIcon}<img class="model-icon" src={aiModelIcon} alt="" />{/if}
              <input
                bind:value={aiModel}
                aria-label="Conversation Model"
                disabled={!aiKeyReady}
                placeholder="model name"
                class:has-icon={Boolean(aiModelIcon)}
                class:invalid={Boolean(aiErrors.model)}
                aria-invalid={Boolean(aiErrors.model)}
                oninput={() => (aiErrors = { ...aiErrors, model: "" })}
                onchange={() => void saveAi()}
                onfocus={() => {
                  if (!aiModelsLoading && aiKeyReady) {
                    void loadModels();
                  }
                }}
              />
            </div>
          {/if}
          {#if aiErrors.model}<small class="field-error">{aiErrors.model}</small>{/if}
          {#if aiModelsLoading}
            <small class="help">Loading models…</small>
          {:else if aiModelsError}
            <small class="help">{aiModelsError}</small>
          {/if}
        </div>

      </section>
    </div>

    {#if aiConfigured}
      {@const embeddingProviderIcons = providerIconUrls(embeddingProvider)}
      <div class="settings-label">Related mail</div>
      <div class="ai-grid">
        <section class="settings-panel ai-panel">
          <div class="panel-head">
            <h4>Provider</h4>
            <button
              class="icon-btn scan-btn"
              class:scanning={aiScanning}
              type="button"
              aria-label="Scan for local embedding providers"
              title="Scan for local providers"
              disabled={aiScanning}
              onclick={() => void scanLocalProviders()}
            >
              <RefreshCw size={14} />
            </button>
          </div>
          <div class="provider-select">
            <button
              class="provider-trigger"
              class:open={embeddingProviderMenuOpen}
              type="button"
              aria-label="Embedding provider"
              aria-haspopup="listbox"
              aria-expanded={embeddingProviderMenuOpen}
              onclick={() => {
                embeddingModelMenuOpen = false;
                embeddingProviderMenuOpen = !embeddingProviderMenuOpen;
              }}
            >
              <span
                class="provider-icon-wrap"
                class:has-dark={Boolean(embeddingProviderIcons.dark)}
              >
                {#if embeddingProviderIcons.light}<img
                    class="provider-icon icon-light"
                    src={embeddingProviderIcons.light}
                    alt=""
                  />{/if}
                {#if embeddingProviderIcons.dark}<img
                    class="provider-icon icon-dark"
                    src={embeddingProviderIcons.dark}
                    alt=""
                  />{/if}
              </span>
              <span class="provider-label">{embeddingProviderInfo.label}</span>
              <ChevronDown size={14} />
            </button>
            {#if embeddingProviderMenuOpen}
              <button
                class="provider-backdrop"
                type="button"
                aria-label="Close embedding provider menu"
                onclick={() => (embeddingProviderMenuOpen = false)}
              ></button>
              <div
                class="provider-menu"
                role="listbox"
                aria-label="Embedding provider"
                tabindex="-1"
                onkeydown={(event) => {
                  if (event.key === "Escape") {
                    embeddingProviderMenuOpen = false;
                    event.stopPropagation();
                  }
                }}
              >
                {#if detectedProviders.length > 0}
                  <div class="menu-group-label">Detected locally</div>
                  {#each detectedProviders as provider (provider.id)}
                    {@render embeddingProviderOption(provider)}
                  {/each}
                  <div class="menu-separator"></div>
                  <div class="menu-group-label">Other providers</div>
                {/if}
                {#each otherProviders as provider (provider.id)}
                  {@render embeddingProviderOption(provider)}
                {/each}
              </div>
            {/if}
          </div>
          {#if embeddingShowEndpoint}
            <label class="field ai-field">
              <span>Base URL</span>
              <input
                bind:value={aiEmbeddingBaseUrl}
                aria-label="Embedding base URL"
                placeholder="http://localhost:11434/v1"
                onchange={() => void saveAi()}
              />
            </label>
          {/if}
          {#if embeddingShowKey}
            <label class="field ai-field">
              <span>API key</span>
              <div class="secret-row">
                <input
                  type={showEmbeddingKey ? "text" : "password"}
                  bind:value={aiEmbeddingKey}
                  aria-label="Embedding API key"
                  autocomplete="off"
                  placeholder={embeddingHasKey ? "••••••••••••" : "Optional for hosted endpoints"}
                  onchange={() => void saveAi()}
                />
                <button
                  class="icon-btn"
                  type="button"
                  aria-label={showEmbeddingKey ? "Hide embedding key" : "Show embedding key"}
                  title={showEmbeddingKey ? "Hide key" : "Show key"}
                  onclick={() => (showEmbeddingKey = !showEmbeddingKey)}
                >
                  {#if showEmbeddingKey}<EyeOff size={15} />{:else}<Eye size={15} />{/if}
                </button>
              </div>
              {#if embeddingHasKey}
                <small class="help key-saved"><Check size={12} /> Key saved</small>
              {/if}
            </label>
            {#if embeddingHasKey}
              <button
                class="ghost"
                type="button"
                disabled={aiSaving}
                onclick={() => void saveAi("")}
              >
                <Trash2 size={15} />
                Remove embedding key
              </button>
            {/if}
          {/if}
        </section>

        <section class="settings-panel ai-panel">
          <h4>Embedding Model</h4>
          <div class="field">
            {#if embeddingModels.length > 0}
              <div class="provider-select">
                <button
                  class="provider-trigger"
                  class:open={embeddingModelMenuOpen}
                  type="button"
                  aria-label="Embedding Model"
                  aria-haspopup="listbox"
                  aria-expanded={embeddingModelMenuOpen}
                  onclick={() => {
                    embeddingProviderMenuOpen = false;
                    embeddingModelMenuOpen = !embeddingModelMenuOpen;
                  }}
                >
                  {#if embeddingModelIcon}<img
                      class="model-icon"
                      src={embeddingModelIcon}
                      alt=""
                    />{/if}
                  <span class="provider-label">
                    {aiEmbeddingModel || "Select model"}
                  </span>
                  <ChevronDown size={14} />
                </button>
                {#if embeddingModelMenuOpen}
                  <button
                    class="provider-backdrop"
                    type="button"
                    aria-label="Close embedding model menu"
                    onclick={() => (embeddingModelMenuOpen = false)}
                  ></button>
                  <div
                    class="provider-menu"
                    role="listbox"
                    aria-label="Embedding Model"
                    tabindex="-1"
                    onkeydown={(event) => {
                      if (event.key === "Escape") {
                        embeddingModelMenuOpen = false;
                        event.stopPropagation();
                      }
                    }}
                  >
                    {#each embeddingModels as model (model)}
                      <button
                        type="button"
                        class:active={aiEmbeddingModel === model}
                        role="option"
                        aria-selected={aiEmbeddingModel === model}
                        onclick={() => {
                          aiEmbeddingModel = model;
                          embeddingModelMenuOpen = false;
                          void saveAi();
                        }}
                      >
                        <span class="provider-label">{model}</span>
                        {#if aiEmbeddingModel === model}<Check size={14} />{/if}
                      </button>
                    {/each}
                  </div>
                {/if}
              </div>
            {:else}
              <div class="input-with-icon">
                {#if embeddingModelIcon}<img
                    class="model-icon"
                    src={embeddingModelIcon}
                    alt=""
                  />{/if}
                <input
                  bind:value={aiEmbeddingModel}
                  aria-label="Embedding Model"
                  class:has-icon={Boolean(embeddingModelIcon)}
                  placeholder="embedding model"
                  onchange={() => void saveAi()}
                />
              </div>
            {/if}
            {#if embeddingModelsLoading}
              <small class="help">Loading models…</small>
            {:else if embeddingModelsError}
              <small class="help">{embeddingModelsError}</small>
            {/if}
          </div>
        </section>
      </div>
      <div class="semantic-footer">
        <div
          class="semantic-status"
          class:ready={semanticStatus?.configured &&
            !semanticStatus?.indexing &&
            semanticStatus?.pending === 0}
        >
          {#if semanticStatus?.indexing}
            <span class="semantic-spinner"><LoaderCircle size={13} /></span>
            <span>
              Indexing… {semanticStatus.indexed} indexed · {semanticStatus.pending}
              pending
            </span>
          {:else if semanticStatus?.configured}
            <Check size={13} />
            <span>
              {semanticStatus.pending > 0 ? "Indexed" : "Ready"}
              · {semanticStatus.indexed} messages indexed
              {#if semanticStatus.pending > 0}
                · {semanticStatus.pending} pending
              {/if}
            </span>
          {:else}
            <span>Set an embedding provider and model to enable related mail.</span>
          {/if}
        </div>
        {#if semanticError}
          <p class="voice-error" role="alert">
            <CircleAlert size={13} />{semanticError}
          </p>
        {/if}
      </div>
    {/if}

    <div class="settings-label">Classification &amp; context</div>
    <section class="settings-panel">
      <div class="settings-row">
        <strong>Classify new mail automatically</strong>
        <ToggleSwitch
          checked={preferences.auto_classify}
          label="Classify new mail automatically"
          onchange={setAutoClassify}
        />
      </div>
      <div class="settings-row">
        <strong>Prepare Aura for new mail</strong>
        <ToggleSwitch
          checked={preferences.auto_rail}
          label="Prepare Aura for new mail"
          onchange={setAutoRail}
        />
      </div>
      <div class="settings-row">
        <strong>Share full message bodies</strong>
        <ToggleSwitch
          checked={preferences.full_body_context}
          label="Share full message bodies"
          onchange={setFullBodyContext}
        />
      </div>
    </section>

    {#if aiConfigured}
      <div class="settings-label">Writing style</div>
      <section class="settings-panel">
        <div class="settings-row">
          <strong>Match my writing</strong>
          <ToggleSwitch
            checked={preferences.match_voice}
            label="Match my writing"
            onchange={setMatchVoice}
          />
        </div>

        <div class="voice-block">
          {#if voiceLoading}
            <p class="voice-loading" role="status">
              <span class="voice-spinner"><LoaderCircle size={13} /></span>
              Loading style…
            </p>
          {:else if voiceProfile}
            <p class="voice-text">{voiceProfile.text}</p>
            <div class="voice-actions">
              <button
                class="ghost"
                type="button"
                disabled={voiceLearning || voiceClearing}
                onclick={() => void clearVoice()}
              >
                <Trash2 size={15} />
                {voiceClearing ? "Clearing…" : "Clear style"}
              </button>
            </div>
          {:else if voiceLearning}
            <p class="voice-loading" role="status">
              <span class="voice-spinner"><LoaderCircle size={13} /></span>
              Learning from Sent mail…
            </p>
          {/if}
          {#if voiceNotice}
            <p class="voice-notice" role="status">
              <Check size={13} />{voiceNotice}
            </p>
          {/if}
          {#if voiceError}
            <p class="voice-error" role="alert">
              <CircleAlert size={13} />{voiceError}
            </p>
          {/if}
        </div>
      </section>
    {/if}

    <div class="form-actions ai-actions">
      {#if aiShowKey && providerHasKey}
        <button class="ghost" type="button" disabled={aiSaving} onclick={() => void clearAiKey()}>
          <Trash2 size={15} />
          Remove key
        </button>
      {/if}
      <button class="ghost" type="button" disabled={aiSaving || aiTesting} onclick={() => void testAi()}>
        <RefreshCw size={15} />
        {aiTesting ? "Testing…" : "Test connection"}
      </button>
    </div>
    {/if}
  {/if}
</div>

<style>
  .edit-space-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
    color: var(--muted);
    font-size: 10.5px;
  }
  .edit-space-row :global(.themed-select) {
    width: 170px;
  }
  .ai-enable-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    margin-bottom: 14px;
    padding: 11px 13px;
    border: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    border-radius: 9px;
    background: color-mix(in srgb, var(--bg-raised) 55%, var(--bg));
  }
  .ai-enable-row strong {
    color: var(--text-strong);
    font-size: 11px;
    font-weight: 650;
  }
  .status-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
    padding: 4px 8px;
    border-radius: 999px;
    background: var(--bg-hover);
    color: var(--muted);
    font-size: 10.5px;
    font-weight: 600;
    white-space: nowrap;
    text-transform: none;
  }
  .status-pill.on {
    background: color-mix(in srgb, var(--success) 14%, transparent);
    color: var(--success);
  }
  .ai-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.1fr) minmax(0, 0.9fr);
    gap: 12px;
    align-items: start;
  }
  .ai-panel {
    padding-bottom: 16px;
  }
  .ai-panel > h4 + .field {
    margin-top: 9px;
  }
  .ai-field {
    margin-top: 12px;
  }
  .panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .panel-head h4 {
    margin: 0;
    color: var(--text);
    font-size: 11.5px;
    font-weight: 620;
  }
  .scan-btn {
    width: 26px;
    height: 26px;
  }
  .scan-btn.scanning :global(svg) {
    animation: voice-spin 900ms linear infinite;
  }
  .provider-select {
    position: relative;
    margin-top: 9px;
  }
  .provider-trigger {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 32px;
    padding: 0 9px;
    border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
    border-radius: 7px;
    background: var(--bg-raised);
    color: var(--text);
    font-size: 12.5px;
    text-align: left;
  }
  .provider-trigger :global(svg) {
    margin-left: auto;
    color: var(--muted);
    transition: transform 140ms ease;
  }
  .provider-trigger.open :global(svg) {
    transform: rotate(180deg);
  }
  .provider-trigger:hover:not(:disabled),
  .provider-trigger:focus-visible:not(:disabled) {
    border-color: color-mix(in srgb, var(--accent) 35%, var(--border));
    background: color-mix(in srgb, var(--bg-hover) 45%, var(--bg-raised));
  }
  .provider-trigger:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .provider-label {
    flex: 1;
    min-width: 0;
  }
  .provider-icon-wrap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    flex: none;
  }
  .provider-icon {
    width: 18px;
    height: 18px;
    object-fit: contain;
  }
  .provider-icon.icon-dark {
    display: none;
  }
  :global(html[data-theme-dark="true"]) .provider-icon-wrap.has-dark .provider-icon.icon-light {
    display: none;
  }
  :global(html[data-theme-dark="true"]) .provider-icon-wrap.has-dark .provider-icon.icon-dark {
    display: block;
  }
  .provider-backdrop {
    position: fixed;
    inset: 0;
    z-index: 1;
    background: transparent;
  }
  .provider-menu {
    position: absolute;
    top: calc(100% + 5px);
    left: 0;
    right: 0;
    z-index: 2;
    max-height: min(320px, 55vh);
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 5px;
    border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
    border-radius: 9px;
    background: var(--bg-raised);
    box-shadow: 0 14px 34px color-mix(in srgb, #000 24%, transparent);
    animation: menu-in 130ms ease-out both;
  }
  .provider-menu button {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    height: 34px;
    padding: 0 9px;
    border-radius: 6px;
    color: var(--text);
    font-size: 12px;
    text-align: left;
  }
  .provider-menu button:hover,
  .provider-menu button:focus-visible {
    background: var(--bg-hover);
  }
  .provider-menu button.active {
    background: color-mix(in srgb, var(--accent) 9%, transparent);
    color: var(--text-strong);
  }
  .menu-trailing {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    margin-left: auto;
    color: var(--accent);
  }
  .menu-group-label {
    padding: 5px 9px 3px;
    color: var(--muted);
    font-size: 9px;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .menu-separator {
    height: 1px;
    margin: 4px 5px;
    background: color-mix(in srgb, var(--border) 60%, transparent);
  }
  .detected {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--success);
  }
  @keyframes menu-in {
    from {
      opacity: 0;
      transform: translateY(-3px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  .input-with-icon {
    position: relative;
  }
  .input-with-icon .model-icon {
    position: absolute;
    top: 50%;
    left: 9px;
    width: 16px;
    height: 16px;
    transform: translateY(-50%);
    object-fit: contain;
  }
  .input-with-icon input.has-icon {
    padding-left: 31px;
  }
  .model-icon {
    width: 16px;
    height: 16px;
    flex: none;
    object-fit: contain;
  }
  .help {
    margin: 0;
    color: var(--muted);
    font-size: 10.5px;
    line-height: 1.45;
  }
  .key-saved {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--success);
  }
  .secret-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .secret-row input {
    flex: 1;
  }
  .icon-btn {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    flex: none;
    border-radius: 7px;
    color: var(--muted);
  }
  .icon-btn:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .ai-actions {
    margin-top: 18px;
  }
  .semantic-footer {
    margin-top: 12px;
    padding: 0 2px;
  }
  .semantic-status {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 9px;
    color: var(--muted);
    font-size: 9.5px;
    line-height: 1.4;
  }
  .semantic-status.ready {
    color: var(--success);
  }
  .semantic-spinner {
    display: grid;
    place-items: center;
    color: var(--accent);
    animation: voice-spin 900ms linear infinite;
  }
  .voice-block {
    padding: 11px 0 4px;
  }
  .voice-text {
    margin: 7px 0 0;
    color: var(--text);
    font-size: 10.5px;
    line-height: 1.55;
    white-space: pre-wrap;
  }
  .voice-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 10px;
  }
  .voice-loading {
    margin: 0;
    color: var(--muted);
    font-size: 9.5px;
    line-height: 1.5;
  }
  .voice-loading {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .voice-spinner,
  .ai-spinner {
    display: grid;
    place-items: center;
    animation: voice-spin 900ms linear infinite;
  }
  .loading-line {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  @keyframes voice-spin {
    to {
      transform: rotate(360deg);
    }
  }
  .voice-notice,
  .voice-error {
    display: flex;
    align-items: center;
    gap: 5px;
    margin: 9px 0 0;
    font-size: 9.5px;
    line-height: 1.4;
  }
  .voice-notice {
    color: var(--success);
  }
  .voice-error {
    color: var(--danger);
  }
  @media (max-width: 820px) {
    .ai-grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
