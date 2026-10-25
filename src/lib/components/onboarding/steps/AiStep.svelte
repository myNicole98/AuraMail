<script lang="ts">
  import { onMount } from "svelte";
  import {
    BookOpen,
    Check,
    CircleAlert,
    Eye,
    EyeOff,
    FileText,
    RefreshCw
  } from "lucide-svelte";
  import {
    providerById,
    providerForUrl,
    type AiProviderId
  } from "../../../ai/providers";
  import { api } from "../../../api";
  import {
    loadPreferences,
    updatePreferences,
    validateAiSettings
  } from "../../../settings";
  import type {
    AiConfig,
    AiConfigInput,
    DiscoveredProvider
  } from "../../../types";
  import AiModelPicker from "../../ai/AiModelPicker.svelte";
  import AiProviderPicker from "../../ai/AiProviderPicker.svelte";
  import { errorMessage } from "../../settings/errors";
  import ToggleSwitch from "../../ToggleSwitch.svelte";
  import StepHeader from "../StepHeader.svelte";

  let {
    onBack,
    onNext
  }: { onBack: () => void; onNext: () => void } = $props();

  const initial = loadPreferences();
  let autoClassify = $state(initial.auto_classify);
  let fullBodyContext = $state(initial.full_body_context);
  let error = $state<string | null>(null);
  let provider = $state<AiProviderId>("openai");
  let baseUrl = $state(providerById("openai").baseUrl);
  let model = $state("");
  let apiKey = $state("");
  let config = $state<AiConfig | null>(null);
  let providerHasKey = $state(false);
  let showKey = $state(false);
  let providerReady = $state(false);
  let savingProvider = $state(false);
  let providerNotice = $state<string | null>(null);
  let providerError = $state<string | null>(null);
  let providerShake = $state(false);
  let shakeTimer: ReturnType<typeof setTimeout> | undefined;
  let models = $state<string[]>([]);
  let modelsLoading = $state(false);
  let modelsError = $state<string | null>(null);
  let discovered = $state<DiscoveredProvider[]>([]);
  let scanning = $state(false);
  let embeddingProvider = $state<AiProviderId>("custom");
  let embeddingBaseUrl = $state("");
  let embeddingModel = $state("");
  let embeddingKey = $state("");
  let embeddingHasKey = $state(false);
  let embeddingModels = $state<string[]>([]);
  let embeddingModelsLoading = $state(false);
  let embeddingModelsError = $state<string | null>(null);
  let showEmbeddingKey = $state(false);

  let providerInfo = $derived(providerById(provider));
  let configuredProvider = $derived(
    config ? providerForUrl(config.base_url) : null
  );
  let providerConfigured = $derived(
    Boolean(config?.configured && configuredProvider === provider)
  );
  let showEndpoint = $derived(!providerInfo.fixed || provider === "custom");
  let showKeyField = $derived(
    providerInfo.requiresKey || provider === "custom"
  );
  let keyPlaceholder = $derived(
    providerHasKey ? "Saved key — leave blank to keep" : providerInfo.keyPlaceholder
  );
  let embeddingProviderInfo = $derived(providerById(embeddingProvider));
  let embeddingShowEndpoint = $derived(
    !embeddingProviderInfo.fixed || embeddingProvider === "custom"
  );
  let embeddingUsesChatKey = $derived(
    embeddingBaseUrl.trim().replace(/\/+$/, "") !== "" &&
      embeddingBaseUrl.trim().replace(/\/+$/, "") ===
        baseUrl.trim().replace(/\/+$/, "")
  );
  let embeddingShowKey = $derived(
    !embeddingUsesChatKey &&
      embeddingProvider !== "ollama" &&
      embeddingProvider !== "lmstudio" &&
      (embeddingProviderInfo.requiresKey || embeddingProvider === "custom")
  );
  let embeddingKeyPlaceholder = $derived(
    embeddingHasKey
      ? "Saved key — leave blank to keep"
      : embeddingProviderInfo.keyPlaceholder || "API key (optional)"
  );

  function isTauri(): boolean {
    return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  }

  function applyConfig(next: AiConfig) {
    config = next;
    provider = providerForUrl(next.base_url);
    baseUrl = next.base_url || providerById(provider).baseUrl;
    model = next.model;
    providerHasKey = next.has_key;
    apiKey = "";
    embeddingProvider = next.embedding_base_url.trim()
      ? providerForUrl(next.embedding_base_url)
      : "custom";
    embeddingBaseUrl = next.embedding_base_url;
    embeddingModel = next.embedding_model;
    embeddingHasKey = next.has_embedding_key;
    embeddingKey = "";
  }

  async function update(patch: {
    auto_classify?: boolean;
    full_body_context?: boolean;
  }) {
    const next = updatePreferences(patch);
    autoClassify = next.auto_classify;
    fullBodyContext = next.full_body_context;
    if (!isTauri()) return;
    try {
      const stored = await api.setAiPreferences({
        auto_classify: next.auto_classify,
        auto_rail: next.auto_rail,
        full_body_context: next.full_body_context,
        match_voice: next.match_voice
      });
      const merged = updatePreferences(stored);
      autoClassify = merged.auto_classify;
      fullBodyContext = merged.full_body_context;
      error = null;
    } catch (caught) {
      error = errorMessage(caught);
    }
  }

  function shakeProvider() {
    clearTimeout(shakeTimer);
    providerShake = false;
    requestAnimationFrame(() => {
      providerShake = true;
      shakeTimer = setTimeout(() => (providerShake = false), 420);
    });
  }

  async function scanLocalProviders(): Promise<DiscoveredProvider[]> {
    if (!isTauri()) return discovered;
    if (scanning) return discovered;
    scanning = true;
    try {
      discovered = await api.discoverAiProviders().catch(() => []);
      return discovered;
    } finally {
      scanning = false;
    }
  }

  async function loadModels() {
    modelsError = null;
    if (!baseUrl.trim()) {
      models = [];
      return;
    }
    if (providerInfo.requiresKey && !providerHasKey && !apiKey.trim()) {
      modelsError = "Add an API key to load models.";
      return;
    }
    modelsLoading = true;
    try {
      const loaded = await api.listAiModels(
        baseUrl.trim(),
        apiKey.trim() || undefined
      );
      models = loaded;
      if (loaded.length > 0 && !loaded.includes(model)) model = loaded[0];
    } catch (caught) {
      models = [];
      modelsError = errorMessage(caught);
    } finally {
      modelsLoading = false;
    }
  }

  async function loadEmbeddingModels() {
    embeddingModelsError = null;
    const base = embeddingBaseUrl.trim();
    if (!base) {
      embeddingModels = [];
      return;
    }
    const usesChatKey = embeddingUsesChatKey;
    const key = usesChatKey ? apiKey.trim() : embeddingKey.trim();
    const hasKey = usesChatKey
      ? providerHasKey || Boolean(apiKey.trim())
      : embeddingHasKey || Boolean(embeddingKey.trim());
    if (embeddingProviderInfo.requiresKey && !hasKey) {
      embeddingModelsError = "Add an embedding API key to load models.";
      return;
    }
    embeddingModelsLoading = true;
    try {
      const loaded = await api.listAiModels(base, key || undefined);
      embeddingModels = loaded.filter((option) =>
        /embed|bge|e5|nomic|mxbai|minilm/i.test(option)
      );
      if (!embeddingModel && embeddingModels.length > 0) {
        embeddingModel = embeddingModels[0];
      }
    } catch (caught) {
      embeddingModels = [];
      embeddingModelsError = errorMessage(caught);
    } finally {
      embeddingModelsLoading = false;
    }
  }

  async function selectProvider(next: AiProviderId) {
    provider = next;
    const info = providerById(next);
    baseUrl = info.baseUrl;
    model = "";
    apiKey = "";
    models = [];
    modelsError = null;
    providerNotice = null;
    providerError = null;
    providerReady = false;
    providerHasKey = false;
    if (isTauri()) {
      providerHasKey = await api.aiHasKey(info.baseUrl).catch(() => false);
    }
    const local = discovered.find((candidate) => candidate.id === next);
    if (local?.models.length) {
      models = local.models;
      model = local.models[0];
    } else if (!info.requiresKey || providerHasKey || apiKey.trim()) {
      void loadModels();
    }
  }

  async function selectEmbeddingProvider(next: AiProviderId) {
    embeddingProvider = next;
    const info = providerById(next);
    embeddingBaseUrl = next === "custom" ? "" : info.baseUrl;
    embeddingModel = "";
    embeddingModels = [];
    embeddingModelsError = null;
    embeddingKey = "";
    embeddingHasKey = false;
    if (isTauri() && next !== "custom") {
      embeddingHasKey = await api.aiHasKey(info.baseUrl).catch(() => false);
    }
    if (next !== "custom") void loadEmbeddingModels();
  }

  async function saveProvider() {
    const errors = validateAiSettings(baseUrl, model);
    providerError = Object.values(errors)[0] ?? null;
    providerNotice = null;
    if (providerError) {
      shakeProvider();
      return;
    }
    if (providerInfo.requiresKey && !providerHasKey && !apiKey.trim()) {
      providerError = "Enter an API key.";
      shakeProvider();
      return;
    }
    const hasEmbedding = embeddingModel.trim().length > 0;
    if (hasEmbedding && !embeddingBaseUrl.trim()) {
      providerError = "Enter an embedding base URL.";
      shakeProvider();
      return;
    }
    if (
      hasEmbedding &&
      embeddingShowKey &&
      !embeddingHasKey &&
      !embeddingKey.trim()
    ) {
      providerError = "Enter an embedding API key.";
      shakeProvider();
      return;
    }
    if (!isTauri()) {
      providerError = "Provider setup is available in the desktop app.";
      shakeProvider();
      return;
    }
    savingProvider = true;
    try {
      const input: AiConfigInput = {
        base_url: baseUrl.trim(),
        model: model.trim(),
        embedding_base_url: hasEmbedding ? embeddingBaseUrl.trim() : "",
        embedding_model: hasEmbedding ? embeddingModel.trim() : "",
        extra_headers: []
      };
      if (apiKey.trim()) input.api_key = apiKey.trim();
      if (hasEmbedding && !embeddingUsesChatKey && embeddingKey.trim()) {
        input.embedding_api_key = embeddingKey.trim();
      }
      applyConfig(await api.setAiConfig(input));
      providerNotice = "Provider saved.";
      providerError = null;
      try {
        await api.testAi();
        providerReady = true;
        providerNotice = "Provider saved and connection verified.";
      } catch (caught) {
        providerReady = false;
        providerError = `Provider saved, but the connection test failed: ${errorMessage(caught)}`;
        shakeProvider();
      }
    } catch (caught) {
      providerError = errorMessage(caught);
      shakeProvider();
    } finally {
      savingProvider = false;
    }
  }

  onMount(() => {
    if (!isTauri()) return;
    void api
      .getAiPreferences()
      .then((stored) => {
        const merged = updatePreferences(stored);
        autoClassify = merged.auto_classify;
        fullBodyContext = merged.full_body_context;
      })
      .catch(() => {});
    void (async () => {
      const stored = await api.getAiConfig().catch(() => null);
      const local = await scanLocalProviders();
      if (stored) {
        applyConfig(stored);
        providerReady = stored.configured;
        if (
          stored.configured ||
          stored.has_key ||
          !providerById(provider).requiresKey
        ) {
          void loadModels();
        }
        void loadEmbeddingModels();
      }
      if (
        !stored?.configured &&
        !stored?.has_key &&
        !stored?.base_url?.trim() &&
        local.length > 0
      ) {
        const preferred = local[0].id as AiProviderId;
        await selectProvider(preferred);
        if (
          !stored?.embedding_base_url?.trim() &&
          !stored?.embedding_model?.trim()
        ) {
          await selectEmbeddingProvider(preferred);
        }
      }
    })();
  });
</script>

<div class="ai-step">
  <StepHeader
    kicker="Aura AI"
    title="Let Aura help."
    description="Optional. Connect a provider, or skip."
  />

  <section
    class="provider-form"
    class:invalid={Boolean(providerError)}
    class:shake={providerShake}
    aria-label="Provider setup"
  >
    <div class="form-head">
      <span class="form-title">Configure provider</span>
      <div class="head-actions">
        <button
          class="scan-btn"
          class:scanning
          type="button"
          aria-label="Scan for local providers"
          title="Scan for local providers"
          disabled={scanning}
          onclick={() => void scanLocalProviders()}
        >
          <RefreshCw size={13} />
        </button>
        {#if providerConfigured}<span class="ready-pill">Ready</span>{/if}
      </div>
    </div>
    <div class="form-grid">
      <div class="field">
        <span>Provider</span>
        <AiProviderPicker
          value={provider}
          detected={discovered}
          label="Provider"
          onselect={selectProvider}
        />
      </div>
      <div class="field">
        <span>Model</span>
        <AiModelPicker
          bind:value={model}
          models={models}
          loading={modelsLoading}
          error={modelsError}
          label="Model"
          placeholder="gpt-4o-mini"
          refreshLabel="Refresh models"
          onload={loadModels}
          onrefresh={loadModels}
        />
      </div>
      {#if showEndpoint}
        <label class="field wide">
          <span>Base URL</span>
          <input
            bind:value={baseUrl}
            placeholder="https://api.example.com/v1"
            onchange={() => void loadModels()}
          />
        </label>
      {/if}
      {#if showKeyField}
        <label class="field wide">
          <span>API key</span>
          <div class="secret-row">
            <input
              type={showKey ? "text" : "password"}
              bind:value={apiKey}
              placeholder={keyPlaceholder}
              autocomplete="off"
            />
            <button
              class="secret-toggle"
              type="button"
              aria-label={showKey ? "Hide API key" : "Show API key"}
              title={showKey ? "Hide key" : "Show key"}
              onclick={() => (showKey = !showKey)}
            >
              {#if showKey}<EyeOff size={14} />{:else}<Eye size={14} />{/if}
            </button>
          </div>
          {#if providerHasKey}<small class="key-saved"><Check size={11} /> Key saved</small>{/if}
        </label>
      {/if}
    </div>

    <div class="form-divider">
      <span>Related mail</span>
      <small>Optional</small>
    </div>
    <div class="form-grid">
      <div class="field">
        <span>Embedding provider</span>
        <AiProviderPicker
          value={embeddingProvider}
          detected={discovered}
          label="Embedding provider"
          onselect={selectEmbeddingProvider}
        />
      </div>
      <div class="field">
        <span>Embedding model</span>
        <AiModelPicker
          bind:value={embeddingModel}
          models={embeddingModels}
          loading={embeddingModelsLoading}
          error={embeddingModelsError}
          label="Embedding model"
          placeholder="text-embedding-3-small"
          refreshLabel="Refresh embedding models"
          onload={loadEmbeddingModels}
          onrefresh={loadEmbeddingModels}
        />
      </div>
      {#if embeddingShowEndpoint}
        <label class="field wide">
          <span>Embedding base URL</span>
          <input
            bind:value={embeddingBaseUrl}
            placeholder="http://localhost:11434/v1"
            onchange={() => void loadEmbeddingModels()}
          />
        </label>
      {/if}
      {#if embeddingShowKey}
        <label class="field wide">
          <span>Embedding API key</span>
          <div class="secret-row">
            <input
              type={showEmbeddingKey ? "text" : "password"}
              bind:value={embeddingKey}
              placeholder={embeddingKeyPlaceholder}
              autocomplete="off"
            />
            <button
              class="secret-toggle"
              type="button"
              aria-label={showEmbeddingKey ? "Hide embedding key" : "Show embedding key"}
              title={showEmbeddingKey ? "Hide key" : "Show key"}
              onclick={() => (showEmbeddingKey = !showEmbeddingKey)}
            >
              {#if showEmbeddingKey}<EyeOff size={14} />{:else}<Eye size={14} />{/if}
            </button>
          </div>
          {#if embeddingHasKey}<small class="key-saved"><Check size={11} /> Key saved</small>{/if}
        </label>
      {/if}
    </div>
    {#if embeddingUsesChatKey}
      <p class="embedding-note">Uses your provider key and endpoint.</p>
    {/if}

    <div class="provider-foot">
      {#if providerNotice}<p class="provider-notice" role="status">{providerNotice}</p>{/if}
      {#if providerError}<p class="provider-error" role="alert">{providerError}</p>{/if}
      <button class="save" type="button" disabled={savingProvider} onclick={() => void saveProvider()}>
        {savingProvider ? "Checking…" : "Save & test"}
      </button>
    </div>
  </section>

  <div class="ai-card">
    <div class="switch-row">
      <span class="icon"><BookOpen size={15} /></span>
      <span class="copy">
        <strong>Automatic classification</strong>
        <small>Sort new mail as it arrives.</small>
      </span>
      <ToggleSwitch
        checked={autoClassify}
        label="Automatic classification"
        onchange={(value) => void update({ auto_classify: value })}
      />
    </div>
    <div class="switch-row">
      <span class="icon"><FileText size={15} /></span>
      <span class="copy">
        <strong>Full message context</strong>
        <small>Let Aura read full messages.</small>
      </span>
      <ToggleSwitch
        checked={fullBodyContext}
        label="Full message context"
        onchange={(value) => void update({ full_body_context: value })}
      />
    </div>
  </div>

  <p class="privacy-note">Off unless you turn them on.</p>
  {#if error}<p class="error" role="alert"><CircleAlert size={13} />{error}</p>{/if}
  <div class="actions">
    <button class="ghost" type="button" onclick={onBack}>Back</button>
    <span class="spacer"></span>
    <button class="ghost" type="button" onclick={onNext}>Skip</button>
    <button class="primary" type="button" disabled={!providerReady} onclick={onNext}>
      Continue
    </button>
  </div>
</div>

<style>
  .ai-step { display: flex; flex-direction: column; gap: 11px; }
  .ai-card,
  .provider-form {
    overflow: visible;
    border: 1px solid color-mix(in srgb, var(--border) 72%, transparent);
    border-radius: 12px;
    background: color-mix(in srgb, var(--bg-raised) 72%, var(--bg));
  }
  .switch-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 13px 14px;
  }
  .switch-row + .switch-row {
    border-top: 1px solid color-mix(in srgb, var(--border) 58%, transparent);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex: none;
    border-radius: 8px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    color: var(--accent);
  }
  .copy { flex: 1; min-width: 0; }
  .copy strong { display: block; color: var(--text-strong); font-size: 11px; }
  .copy small { display: block; margin-top: 2px; color: var(--muted); font-size: 9px; }
  .form-head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
  }
  .form-title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-strong);
    font-size: 11px;
    font-weight: 650;
  }
  .head-actions { display: flex; align-items: center; gap: 7px; margin-left: auto; }
  .scan-btn {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    color: var(--muted);
  }
  .scan-btn:hover:not(:disabled) { color: var(--text); background: var(--bg-hover); }
  .scan-btn:disabled { opacity: 0.55; }
  .scan-btn.scanning :global(svg) { animation: spin 900ms linear infinite; }
  .ready-pill {
    padding: 2px 7px;
    border-radius: 99px;
    background: color-mix(in srgb, var(--success) 14%, transparent);
    color: var(--success);
    font-size: 8px;
    font-weight: 650;
  }
  .form-divider {
    display: flex;
    align-items: baseline;
    gap: 7px;
    margin: 12px 0 8px;
    padding-top: 10px;
    border-top: 1px solid color-mix(in srgb, var(--border) 58%, transparent);
  }
  .form-divider span { color: var(--text-strong); font-size: 10px; font-weight: 650; }
  .form-divider small { color: var(--muted); font-size: 8.5px; }
  .provider-form { padding: 12px 13px; transition: border-color 160ms ease; }
  .provider-form.invalid { border-color: color-mix(in srgb, var(--danger) 70%, var(--border)); }
  .provider-form.shake { animation: provider-shake 380ms ease; }
  @keyframes provider-shake {
    0%, 100% { transform: translateX(0); }
    20% { transform: translateX(-5px); }
    40% { transform: translateX(4px); }
    60% { transform: translateX(-3px); }
    80% { transform: translateX(2px); }
  }
  .form-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 9px; }
  .field { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .field.wide { grid-column: 1 / -1; }
  .field > span { color: var(--muted); font-size: 8.5px; font-weight: 650; letter-spacing: 0.04em; }
  input {
    width: 100%;
    min-height: 31px;
    padding: 6px 9px;
    border: 1px solid color-mix(in srgb, var(--border) 78%, transparent);
    border-radius: 7px;
    background-color: color-mix(in srgb, var(--bg) 55%, var(--bg-raised));
    color: var(--text);
    font-size: 10px;
  }
  .secret-row { display: flex; align-items: center; gap: 6px; }
  .secret-row input { flex: 1; min-width: 0; }
  .secret-toggle {
    display: grid;
    place-items: center;
    width: 31px;
    height: 31px;
    flex: none;
    border: 1px solid color-mix(in srgb, var(--border) 78%, transparent);
    border-radius: 7px;
    color: var(--muted);
  }
  .secret-toggle:hover { color: var(--text); background: var(--bg-hover); }
  .key-saved {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--success);
    font-size: 8.5px;
  }
  .embedding-note {
    margin: 8px 0 0;
    color: var(--muted);
    font-size: 8.5px;
  }
  .provider-foot {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-top: 10px;
  }
  .provider-notice,
  .provider-error {
    flex: 1;
    margin: 0;
    font-size: 9px;
    line-height: 1.4;
  }
  .provider-notice { color: var(--success); }
  .provider-error { color: var(--danger); }
  .save {
    margin-left: auto;
    padding: 7px 11px;
    border-radius: 7px;
    background: var(--accent);
    color: var(--bg);
    font-size: 9.5px;
    font-weight: 650;
  }
  .save:disabled { opacity: 0.55; }
  .privacy-note { margin: 0; color: var(--muted); font-size: 9.5px; line-height: 1.55; }
  .error {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--danger);
    font-size: 9.5px;
  }
  .actions { display: flex; align-items: center; gap: 8px; margin-top: 2px; }
  .spacer { flex: 1; }
  .ghost,
  .primary { padding: 9px 16px; border-radius: 8px; font-size: 10.5px; font-weight: 650; }
  .ghost { border: 1px solid color-mix(in srgb, var(--border) 80%, transparent); color: var(--muted); }
  .primary { background: var(--accent); color: var(--bg); }
  .primary:disabled { opacity: 0.38; cursor: default; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 640px) {
    .form-grid { grid-template-columns: minmax(0, 1fr); }
    .field.wide { grid-column: auto; }
  }
</style>
