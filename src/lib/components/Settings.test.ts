import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within
} from "@testing-library/svelte";
import { tick } from "svelte";
import Settings from "./Settings.svelte";
import AccountSettings from "./settings/AccountSettings.svelte";
import AppearanceSettings from "./settings/AppearanceSettings.svelte";
import GeneralSettings from "./settings/GeneralSettings.svelte";
import AiSettings from "./settings/AiSettings.svelte";
import { loadSpaceAppearance, loadSpaceGeneral } from "../spaces";
import { api } from "../api";
import { loadPreferences } from "../settings";
import { app } from "../stores/app.svelte";
import { appearance } from "../stores/appearance.svelte";
import type { Account } from "../types";

vi.mock("../api", () => ({
  api: {
    getAiPreferences: vi.fn().mockResolvedValue({
      auto_classify: true,
      auto_rail: false,
      full_body_context: false,
      match_voice: false
    }),
    setAiPreferences: vi.fn(),
    getAiVoiceProfile: vi.fn().mockResolvedValue(null),
    learnAiVoice: vi.fn(),
    clearAiVoiceProfile: vi.fn(),
    getAiConfig: vi.fn().mockResolvedValue({
      base_url: "",
      model: "",
      embedding_base_url: "",
      embedding_model: "",
      extra_headers: [],
      enabled: true,
      configured: false,
      has_key: false,
      has_embedding_key: false
    }),
    discoverAiProviders: vi.fn().mockResolvedValue([]),
    listAiModels: vi.fn().mockResolvedValue([]),
    aiHasKey: vi.fn().mockResolvedValue(false),
    setAiEnabled: vi.fn().mockImplementation(async (enabled) => ({
      base_url: "",
      model: "",
      embedding_base_url: "",
      embedding_model: "",
      extra_headers: [],
      enabled,
      configured: false,
      has_key: false,
      has_embedding_key: false
    })),
    aiStatus: vi.fn().mockResolvedValue(false),
    setAiConfig: vi.fn().mockImplementation(async (input) => ({
      ...input,
      enabled: true,
      configured: true,
      has_key: true,
      has_embedding_key: false
    })),
    getSemanticStatus: vi.fn().mockResolvedValue({
      configured: false,
      model: "",
      indexed: 0,
      pending: 0,
      indexing: false,
      error: null
    }),
    startSemanticIndex: vi.fn().mockResolvedValue({
      configured: false,
      model: "",
      indexed: 0,
      pending: 0,
      indexing: false,
      error: null
    }),
    setMailPreferences: vi.fn().mockResolvedValue({}),
    setSyncInterval: vi.fn(),
    setAccountColor: vi.fn(),
    getFastSyncInterval: vi.fn().mockResolvedValue(30),
    setFastSyncInterval: vi.fn(),
    googleOAuthStatus: vi.fn().mockResolvedValue({ configured: true, reason: null }),
    startGoogleOAuth: vi.fn(),
    reconnectGoogleAccount: vi.fn(),
    cancelGoogleOAuth: vi.fn(),
    microsoftOAuthStatus: vi.fn().mockResolvedValue({ configured: true, reason: null }),
    startMicrosoftOAuth: vi.fn(),
    reconnectMicrosoftAccount: vi.fn(),
    cancelMicrosoftOAuth: vi.fn(),
    createSpace: vi.fn(),
    spaceUnreadCounts: vi.fn().mockResolvedValue([]),
    renameSpace: vi.fn(),
    deleteSpace: vi.fn(),
    reorderAccounts: vi.fn(),
    moveAccount: vi.fn(),
    listAccounts: vi.fn().mockResolvedValue([]),
    listFolders: vi.fn().mockResolvedValue([]),
    listDrafts: vi.fn().mockResolvedValue([]),
    listTabs: vi.fn().mockResolvedValue([]),
    listThreads: vi.fn().mockResolvedValue([]),
    tabUnreadCounts: vi.fn().mockResolvedValue({ all: 0, tabs: [] }),
    refreshTrayUnread: vi.fn().mockResolvedValue(undefined)
  }
}));

async function chooseDropdown(label: string, option: string): Promise<void> {
  await fireEvent.click(screen.getByLabelText(label));
  await fireEvent.click(screen.getByRole("option", { name: option }));
}

function manualAccount(overrides: Partial<Account> = {}): Account {
  return {
    id: 1,
    display_name: "Work",
    email: "work@example.com",
    username: "work@example.com",
    imap_host: "imap.example.com",
    imap_port: 993,
    imap_security: "tls",
    smtp_host: "smtp.example.com",
    smtp_port: 465,
    smtp_security: "tls",
    color: "#6da4ff",
    signature: "",
    signature_fields: {
      profession: "",
      phone: "",
      location: "",
      website: "",
      socials: []
    },
    signature_on_new: true,
    auth_type: "password",
    oauth_provider: "",
    space_id: 1,
    position: 0,
    sync_interval_secs: 60,
    created_at: 0,
    ...overrides
  };
}

function googleAccount(overrides: Partial<Account> = {}): Account {
  return manualAccount({
    id: 2,
    display_name: "Me",
    email: "me@gmail.com",
    username: "me@gmail.com",
    imap_host: "imap.gmail.com",
    smtp_host: "smtp.gmail.com",
    signature_on_new: true,
    auth_type: "oauth",
    oauth_provider: "google",
    ...overrides
  });
}

beforeEach(() => {
  app.spaces = [
    { id: 1, name: "Personal", icon: "layers", position: 0, created_at: 0 },
    { id: 2, name: "Work", icon: "layers", position: 1, created_at: 0 }
  ];
  app.activeSpaceId = 1;
});

afterEach(() => {
  cleanup();
  app.accounts = [];
  localStorage.clear();
  document.documentElement.removeAttribute("data-color-vision");
  document.documentElement.removeAttribute("data-theme-background");
  document.documentElement.removeAttribute("data-theme-panes");
  document.documentElement.removeAttribute("data-theme-glass");
  appearance.timeFormat = "system";
  appearance.colorVision = "none";
});

describe("Settings shell", () => {
  it("renders six horizontal icon tabs", () => {
    render(Settings, { props: { onClose: vi.fn() } });
    const tabs = screen.getAllByRole("tab");
    expect(tabs).toHaveLength(6);
    expect(tabs.map((tab) => tab.textContent?.trim())).toEqual([
      "General",
      "Account",
      "Appearance",
      "Accessibility",
      "Signatures",
      "Aura AI"
    ]);
  });

  it("switches panels on click and arrow keys", async () => {
    render(Settings, { props: { onClose: vi.fn() } });

    await fireEvent.click(screen.getByRole("tab", { name: "Account" }));
    await tick();
    expect(screen.getByRole("tab", { name: "Account" })).toHaveAttribute(
      "aria-selected",
      "true"
    );
    expect(screen.getByRole("button", { name: "Create Space" })).toBeInTheDocument();

    await fireEvent.keyDown(screen.getByRole("tab", { name: "Account" }), {
      key: "ArrowLeft"
    });
    await tick();
    expect(screen.getByRole("tab", { name: "General" })).toHaveAttribute(
      "aria-selected",
      "true"
    );
    expect(screen.getByText("Notifications")).toBeInTheDocument();
  });

  it("keeps the style choices free of explanatory copy", async () => {
    render(Settings, { props: { onClose: vi.fn() } });
    await fireEvent.click(screen.getByRole("tab", { name: "Appearance" }));
    await tick();

    expect(
      screen.queryByText(/Solid uses your theme colors/i)
    ).not.toBeInTheDocument();
    expect(
      screen.queryByText(/Straight uses flush 1px dividers/i)
    ).not.toBeInTheDocument();
  });

  it("uses functional settings headings without marketing titles", async () => {
    render(Settings, { props: { onClose: vi.fn() } });

    for (const tab of [
      "General",
      "Account",
      "Appearance",
      "Accessibility",
      "Signatures",
      "Aura AI"
    ]) {
      await fireEvent.click(screen.getByRole("tab", { name: tab }));
      await tick();
      expect(screen.queryByRole("heading", { level: 1 })).not.toBeInTheDocument();
    }
  });


  it("moves accessibility controls out of appearance", async () => {
    render(Settings, { props: { onClose: vi.fn() } });
    await fireEvent.click(screen.getByRole("tab", { name: "Appearance" }));
    await tick();

    expect(screen.queryByText("Disable animations")).not.toBeInTheDocument();
    expect(screen.queryByText("High contrast")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Color vision")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("UI size")).not.toBeInTheDocument();
  });

  it("hosts all accessibility controls in a dedicated tab", async () => {
    render(Settings, { props: { onClose: vi.fn() } });
    await fireEvent.click(screen.getByRole("tab", { name: "Accessibility" }));
    await tick();

    expect(screen.getByLabelText("UI size")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Disable animations" })).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "High contrast" })).toBeInTheDocument();
    expect(screen.getByLabelText("Color vision")).toBeInTheDocument();
  });

  it("switches background and pane choices independently", async () => {
    render(Settings, { props: { onClose: vi.fn() } });
    await fireEvent.click(screen.getByRole("tab", { name: "Appearance" }));
    await tick();

    const backgrounds = screen.getByRole("group", { name: "Background" });
    const panes = screen.getByRole("group", { name: "Pane dividers" });
    expect(within(backgrounds).getByRole("button", { name: "Solid" })).toHaveAttribute(
      "aria-pressed",
      "true"
    );
    expect(within(panes).getByRole("button", { name: "Straight" })).toHaveAttribute(
      "aria-pressed",
      "true"
    );

    await fireEvent.click(within(backgrounds).getByRole("button", { name: "Glass" }));
    await waitFor(() =>
      expect(document.documentElement.dataset.themeBackground).toBe("glass")
    );
    expect(document.documentElement.dataset.themeGlass).toBe("true");
    expect(loadPreferences().theme_background).toBe("glass");

    await fireEvent.click(within(panes).getByRole("button", { name: "Rounded" }));
    await waitFor(() =>
      expect(document.documentElement.dataset.themePanes).toBe("rounded")
    );
    expect(loadPreferences().theme_panes).toBe("rounded");
    expect(loadPreferences().theme_background).toBe("glass");

    await fireEvent.click(within(backgrounds).getByRole("button", { name: "Solid" }));
    await waitFor(() =>
      expect(document.documentElement.dataset.themeBackground).toBe("solid")
    );
    expect(loadPreferences().theme_background).toBe("solid");
    expect(loadPreferences().theme_panes).toBe("rounded");
  });

  it("applies high contrast from accessibility settings", async () => {
    render(Settings, { props: { onClose: vi.fn() } });
    await fireEvent.click(screen.getByRole("tab", { name: "Accessibility" }));
    await tick();

    await fireEvent.click(screen.getByRole("switch", { name: "High contrast" }));
    expect(document.documentElement.dataset.highContrast).toBe("true");
  });

  it("offers and persists the color-vision picker", async () => {
    render(Settings, { props: { onClose: vi.fn() } });
    await fireEvent.click(screen.getByRole("tab", { name: "Accessibility" }));
    await tick();

    expect(screen.getByLabelText("Color vision")).toBeInTheDocument();
    await chooseDropdown("Color vision", "Deuteranopia");

    expect(loadPreferences().color_vision).toBe("deuteranopia");
    expect(document.documentElement.dataset.colorVision).toBe("deuteranopia");
  });

  it("toggles sender profile pictures from privacy", async () => {
    vi.mocked(api.setMailPreferences).mockClear();
    render(Settings, { props: { onClose: vi.fn() } });

    expect(loadPreferences().profile_pictures).toBe(true);
    await fireEvent.click(
      screen.getByRole("switch", { name: "Sender profile pictures" })
    );

    expect(loadPreferences().profile_pictures).toBe(false);
    expect(api.setMailPreferences).toHaveBeenCalledWith(
      expect.objectContaining({ profile_pictures: false }),
      1
    );
  });

  it("keeps mark-as-read per space without switching the active space", async () => {
    app.spaces = [
      { id: 1, name: "Personal", icon: "layers", position: 0, created_at: 0 },
      { id: 2, name: "Work", icon: "briefcase", position: 1, created_at: 0 }
    ];
    app.activeSpaceId = 1;
    render(GeneralSettings);

    await chooseDropdown("Edit space", "Work");
    await chooseDropdown("Mark as read", "Never");

    expect(loadSpaceGeneral(2).mark_read_delay).toBe(-1);
    expect(app.activeSpaceId).toBe(1);
  });

  it("persists the clock format from General", async () => {
    render(Settings, { props: { onClose: vi.fn() } });
    await chooseDropdown("Clock", "24-hour");
    expect(loadPreferences().time_format).toBe("24h");
  });

  it("persists the mark-as-read delay from General", async () => {
    render(Settings, { props: { onClose: vi.fn() } });
    await chooseDropdown("Mark as read", "After 10 seconds");
    expect(loadPreferences().mark_read_delay).toBe(10);
  });

  it("persists conversation order from General", async () => {
    render(Settings, { props: { onClose: vi.fn() } });
    await chooseDropdown("Conversation order", "Oldest first");
    expect(loadPreferences().message_order).toBe("oldest");
  });

  it("persists attachment preferences from General", async () => {
    render(Settings, { props: { onClose: vi.fn() } });
    await fireEvent.click(screen.getByRole("switch", { name: "Show image thumbnails" }));
    expect(loadPreferences().attachment_thumbnails).toBe(false);

    await fireEvent.click(screen.getByRole("switch", { name: "Open folder after saving" }));
    expect(loadPreferences().reveal_download).toBe(true);
  });

  it("hosts automatic classification in Aura AI", async () => {
    render(Settings, { props: { onClose: vi.fn() } });
    expect(screen.queryByText("Auto-classify new mail")).not.toBeInTheDocument();

    await fireEvent.click(screen.getByRole("tab", { name: "Aura AI" }));
    expect(
      await screen.findByRole("switch", {
        name: "Classify new mail automatically"
      })
    ).toBeInTheDocument();
    expect(
      screen.getByRole("switch", { name: "Prepare Aura for new mail" })
    ).toBeInTheDocument();
    expect(
      screen.getByRole("switch", { name: "Share full message bodies" })
    ).toBeInTheDocument();
  });

  it("shows the learned reply voice when Aura is configured", async () => {
    vi.mocked(api.getAiConfig).mockResolvedValueOnce({
      base_url: "https://api.example.com/v1",
      model: "test-model",
      embedding_base_url: "",
      embedding_model: "",
      extra_headers: [],
      enabled: true,
      configured: true,
      has_key: true,
      has_embedding_key: false
    });
    vi.mocked(api.getAiVoiceProfile).mockResolvedValueOnce({
      text: "- Warm greetings\n- Short sentences",
      updatedAt: 1_700_000_000,
      sampleCount: 3
    });
    render(Settings, { props: { onClose: vi.fn() } });

    await fireEvent.click(screen.getByRole("tab", { name: "Aura AI" }));
    await waitFor(() =>
      expect(
        screen.getByRole("switch", { name: "Match my writing" })
      ).toBeInTheDocument()
    );
    expect(await screen.findByText(/Warm greetings/)).toBeInTheDocument();
  });

  it("shows the semantic index status", async () => {
    vi.mocked(api.getAiConfig).mockResolvedValueOnce({
      base_url: "https://api.example.com/v1",
      model: "test-model",
      embedding_base_url: "http://localhost:11434/v1",
      embedding_model: "text-embedding-3-small",
      extra_headers: [],
      enabled: true,
      configured: true,
      has_key: true,
      has_embedding_key: false
    });
    vi.mocked(api.getSemanticStatus).mockResolvedValueOnce({
      configured: true,
      model: "text-embedding-3-small",
      indexed: 12,
      pending: 0,
      indexing: false,
      error: null
    });
    render(Settings, { props: { onClose: vi.fn() } });

    await fireEvent.click(screen.getByRole("tab", { name: "Aura AI" }));
    expect(
      await screen.findByRole("button", { name: "Embedding provider" })
    ).toHaveTextContent("Ollama");
    expect(
      await screen.findByRole("button", { name: "Embedding Model" })
    ).toHaveTextContent("text-embedding-3-small");
    expect(
      await screen.findByText("Ready · 12 messages indexed")
    ).toBeInTheDocument();
  });

  it("opens the provider and model menus", async () => {
    vi.mocked(api.getAiConfig).mockResolvedValueOnce({
      base_url: "https://api.openai.com/v1",
      model: "gpt-4o",
      embedding_base_url: "",
      embedding_model: "",
      extra_headers: [],
      enabled: true,
      configured: true,
      has_key: true,
      has_embedding_key: false
    });
    vi.mocked(api.listAiModels).mockResolvedValueOnce(["gpt-4o", "gpt-4o-mini"]);
    render(Settings, { props: { onClose: vi.fn() } });

    await fireEvent.click(screen.getByRole("tab", { name: "Aura AI" }));
    const provider = await screen.findByRole("button", { name: "OpenAI" });
    await fireEvent.click(provider);
    expect(
      await screen.findByRole("listbox", { name: "AI provider" })
    ).toBeInTheDocument();
    await fireEvent.click(
      screen.getByRole("button", { name: "Close provider menu" })
    );

    const model = await screen.findByRole("button", { name: "Conversation Model" });
    await fireEvent.click(model);
    expect(
      await screen.findByRole("listbox", { name: "Conversation Model" })
    ).toBeInTheDocument();
  });

  it("keeps the detected embedding provider after choosing its model", async () => {
    vi.mocked(api.getAiConfig).mockResolvedValueOnce({
      base_url: "https://api.openai.com/v1",
      model: "gpt-4o",
      embedding_base_url: "",
      embedding_model: "",
      extra_headers: [],
      enabled: true,
      configured: true,
      has_key: true,
      has_embedding_key: false
    });
    vi.mocked(api.discoverAiProviders).mockResolvedValueOnce([
      {
        id: "ollama",
        base_url: "http://localhost:11434/v1",
        models: ["llama3.2", "nomic-embed-text"]
      }
    ]);
    render(Settings, { props: { onClose: vi.fn() } });

    await fireEvent.click(screen.getByRole("tab", { name: "Aura AI" }));
    const provider = await screen.findByRole("button", {
      name: "Embedding provider"
    });
    await fireEvent.click(provider);
    await fireEvent.click(await screen.findByRole("option", { name: /Ollama/ }));

    expect(
      await screen.findByLabelText("Embedding base URL")
    ).toHaveValue("http://localhost:11434/v1");
    expect(
      screen.getByRole("button", { name: "Embedding provider" })
    ).toHaveTextContent("Ollama");
    expect(
      screen.getByRole("button", { name: "Embedding Model" })
    ).toHaveTextContent("nomic-embed-text");
  });

  it("shows detected local providers in the menus and embedding section", async () => {
    vi.mocked(api.getAiConfig).mockResolvedValueOnce({
      base_url: "https://api.openai.com/v1",
      model: "test-model",
      embedding_base_url: "",
      embedding_model: "",
      extra_headers: [],
      enabled: true,
      configured: true,
      has_key: true,
      has_embedding_key: false
    });
    vi.mocked(api.discoverAiProviders).mockResolvedValueOnce([
      {
        id: "ollama",
        base_url: "http://localhost:11434/v1",
        models: ["llama3.2", "nomic-embed-text"]
      }
    ]);
    render(Settings, { props: { onClose: vi.fn() } });

    await fireEvent.click(screen.getByRole("tab", { name: "Aura AI" }));
    const provider = await screen.findByRole("button", { name: "OpenAI" });
    await fireEvent.click(provider);

    expect(await screen.findByText("Detected locally")).toBeInTheDocument();
    await fireEvent.click(
      screen.getByRole("button", { name: "Close provider menu" })
    );

    const embeddingProvider = await screen.findByRole("button", {
      name: "Embedding provider"
    });
    await fireEvent.click(embeddingProvider);
    expect(
      await screen.findByRole("listbox", { name: "Embedding provider" })
    ).toBeInTheDocument();
    expect(
      screen.getByRole("option", { name: /Ollama/ })
    ).toBeInTheDocument();
  });

  it("learns and enables the reply voice", async () => {
    vi.mocked(api.getAiConfig).mockResolvedValueOnce({
      base_url: "https://api.example.com/v1",
      model: "test-model",
      embedding_base_url: "",
      embedding_model: "",
      extra_headers: [],
      enabled: true,
      configured: true,
      has_key: true,
      has_embedding_key: false
    });
    vi.mocked(api.getAiVoiceProfile).mockResolvedValueOnce(null);
    vi.mocked(api.learnAiVoice).mockResolvedValueOnce({
      text: "- Warm greetings\n- Short sentences",
      updatedAt: 1_700_000_000,
      sampleCount: 2
    });
    vi.mocked(api.setAiPreferences).mockClear();
    render(Settings, { props: { onClose: vi.fn() } });

    await fireEvent.click(screen.getByRole("tab", { name: "Aura AI" }));
    const matchVoice = await screen.findByRole("switch", {
      name: "Match my writing"
    });
    expect(
      screen.queryByRole("button", { name: /learn/i })
    ).not.toBeInTheDocument();
    await fireEvent.click(matchVoice);

    expect(await screen.findByText(/Warm greetings/)).toBeInTheDocument();
    expect(api.learnAiVoice).toHaveBeenCalled();
    expect(api.setAiPreferences).toHaveBeenCalledWith(
      expect.objectContaining({ match_voice: true }),
      1
    );
  });

  it("closes on Escape", async () => {
    const onClose = vi.fn();
    render(Settings, { props: { onClose } });
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("creates a space card and keeps the active mail space unchanged", async () => {
    app.accounts = [manualAccount()];
    app.spaces = [{ id: 1, name: "Personal", icon: "layers", position: 0, created_at: 0 }];
    app.activeSpaceId = 1;
    vi.mocked(api.createSpace).mockResolvedValue({
      id: 2,
      name: "Work",
      icon: "layers",
      position: 1,
      created_at: 0
    });

    render(AccountSettings);
    await fireEvent.click(screen.getByRole("button", { name: "Create Space" }));
    await fireEvent.input(screen.getByLabelText("Space name"), {
      target: { value: "Work" }
    });
    await fireEvent.click(screen.getByRole("button", { name: "Create" }));

    expect(api.createSpace).toHaveBeenCalledWith("Work", "layers");
    expect(app.activeSpaceId).toBe(1);
  });

  it("reorders accounts within a space", async () => {
    app.spaces = [{ id: 1, name: "Personal", icon: "layers", position: 0, created_at: 0 }];
    app.accounts = [
      manualAccount({ id: 1, display_name: "A", position: 0 }),
      manualAccount({
        id: 2,
        display_name: "B",
        email: "b@example.com",
        position: 1
      })
    ];
    vi.mocked(api.reorderAccounts).mockResolvedValue([app.accounts[1], app.accounts[0]]);

    render(AccountSettings);
    await fireEvent.click(screen.getByRole("button", { name: "Move B up" }));

    expect(api.reorderAccounts).toHaveBeenCalledWith(1, [2, 1]);
  });

  it("reorders accounts by dragging within a space", async () => {
    app.spaces = [{ id: 1, name: "Personal", icon: "layers", position: 0, created_at: 0 }];
    app.accounts = [
      manualAccount({ id: 1, display_name: "A", position: 0 }),
      manualAccount({
        id: 2,
        display_name: "B",
        email: "b@example.com",
        position: 1
      })
    ];
    vi.mocked(api.reorderAccounts).mockResolvedValue([app.accounts[1], app.accounts[0]]);

    render(AccountSettings);
    const rows = screen.getAllByRole("listitem");
    const targetRow = rows[0];
    vi.spyOn(targetRow, "getBoundingClientRect").mockReturnValue({
      top: 0,
      height: 20,
      bottom: 20,
      left: 0,
      right: 100,
      width: 100,
      x: 0,
      y: 0,
      toJSON: () => ({})
    } as DOMRect);
    await fireEvent.dragStart(rows[1]);
    const dragOver = new Event("dragover", { bubbles: true, cancelable: true });
    Object.defineProperty(dragOver, "clientY", { value: 1 });
    await fireEvent(targetRow, dragOver);
    expect(app.accounts.map((candidate) => candidate.id)).toEqual([2, 1]);
    await fireEvent.drop(targetRow);

    expect(api.reorderAccounts).toHaveBeenCalledWith(1, [2, 1]);
  });

  it("moves an account to another space by dropping it on the card", async () => {
    app.spaces = [
      { id: 1, name: "Personal", icon: "layers", position: 0, created_at: 0 },
      { id: 2, name: "Work", icon: "briefcase", position: 1, created_at: 0 }
    ];
    app.activeSpaceId = 1;
    app.accounts = [
      manualAccount({ id: 1, display_name: "A", position: 0 }),
      manualAccount({
        id: 2,
        display_name: "B",
        email: "b@example.com",
        space_id: 2,
        position: 0
      })
    ];
    const moved = { ...app.accounts[0], space_id: 2, position: 1 };
    vi.mocked(api.moveAccount).mockResolvedValue(moved);
    vi.mocked(api.listAccounts).mockResolvedValue([moved, app.accounts[1]]);

    render(AccountSettings);
    const rows = screen.getAllByRole("listitem");
    const workCard = screen.getByRole("group", { name: "Work" });
    await fireEvent.dragStart(rows[0]);
    await fireEvent.dragOver(workCard);
    expect(workCard).toHaveClass("drop-target");
    await fireEvent.drop(workCard);

    expect(api.moveAccount).toHaveBeenCalledWith(1, 2);
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(app.accounts.find((candidate) => candidate.id === 1)?.space_id).toBe(2);
    expect(screen.getByLabelText("Space")).toHaveTextContent("Work");
  });

  it("switches to a remaining space after deleting the active one", async () => {
    app.spaces = [
      { id: 1, name: "Personal", icon: "layers", position: 0, created_at: 0 },
      { id: 2, name: "Work", icon: "layers", position: 1, created_at: 0 }
    ];
    app.activeSpaceId = 2;
    app.accounts = [manualAccount({ space_id: 2 })];
    vi.mocked(api.deleteSpace).mockResolvedValue([
      { id: 1, name: "Personal", icon: "layers", position: 0, created_at: 0 }
    ]);

    render(AccountSettings);
    await fireEvent.click(screen.getByRole("button", { name: "Delete Work" }));
    await fireEvent.click(screen.getByRole("button", { name: "Delete space" }));

    await waitFor(() => expect(app.activeSpaceId).toBe(1));
  });

  it("opens the provider chooser from Add account", async () => {
    app.accounts = [manualAccount()];
    render(Settings, { props: { onClose: vi.fn() } });
    await fireEvent.click(screen.getByRole("tab", { name: "Account" }));
    await waitFor(() => screen.getAllByRole("button", { name: "Add account" })[0]);
    await fireEvent.click(screen.getAllByRole("button", { name: "Add account" })[0]);
    expect(screen.getByRole("dialog", { name: "Add account" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Google" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "IMAP/SMTP" })).toBeInTheDocument();
  });

  it("shows reconnect instead of password fields for a Google account", async () => {
    app.accounts = [googleAccount()];
    render(Settings, { props: { onClose: vi.fn() } });
    await fireEvent.click(screen.getByRole("tab", { name: "Account" }));
    expect(screen.getByText("Google")).toBeInTheDocument();
    expect(screen.queryByLabelText("Password")).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Account setup" }));
    expect(screen.getByRole("button", { name: "Reconnect Google" }))
      .toBeInTheDocument();
  });

  it("opens the chooser automatically when there are no accounts", async () => {
    app.accounts = [];
    render(Settings, { props: { onClose: vi.fn() } });
    await fireEvent.click(screen.getByRole("tab", { name: "Account" }));
    expect(screen.getByRole("dialog", { name: "Add account" })).toBeInTheDocument();
  });

  it("opens the blank manual form from the chooser", async () => {
    app.accounts = [manualAccount()];
    render(Settings, { props: { onClose: vi.fn() } });
    await fireEvent.click(screen.getByRole("tab", { name: "Account" }));
    await fireEvent.click(screen.getAllByRole("button", { name: "Add account" })[0]);
    await fireEvent.click(screen.getByRole("button", { name: "IMAP/SMTP" }));
    expect(screen.getByRole("dialog", { name: "Add account" })).toBeInTheDocument();
    expect(screen.getByLabelText("IMAP host")).toBeInTheDocument();
    expect(screen.getByLabelText("Password")).toBeInTheDocument();
  });

  it("focuses the requested account by id", async () => {
    app.accounts = [manualAccount(), googleAccount()];
    render(AccountSettings, { props: { focusAccountId: 2 } });
    await fireEvent.click(await screen.findByRole("button", { name: "Account setup" }));
    expect(
      await screen.findByRole("button", { name: "Reconnect Google" })
    ).toBeInTheDocument();
  });

  it("notifies the reader when email dark mode changes", async () => {
    const listener = vi.fn();
    window.addEventListener("auramail:preferences-changed", listener);
    try {
      render(AppearanceSettings);
      await tick();

      await fireEvent.click(
        screen.getByRole("switch", { name: "Use email dark mode" })
      );

      await waitFor(() =>
        expect(loadSpaceAppearance(1).dark_email).toBe(true)
      );
      expect(listener).toHaveBeenCalled();
    } finally {
      window.removeEventListener("auramail:preferences-changed", listener);
    }
  });

  it("edits another space appearance without changing the active space", async () => {
    app.spaces = [
      { id: 1, name: "Personal", icon: "layers", position: 0, created_at: 0 },
      { id: 2, name: "Work", icon: "layers", position: 1, created_at: 0 }
    ];
    app.activeSpaceId = 1;
    render(AppearanceSettings);

    await chooseDropdown("Edit space", "Work");
    expect(
      screen.getByRole("button", { name: "Dark" }).getAttribute("aria-pressed")
    ).toBe("false");
    await fireEvent.click(screen.getByRole("button", { name: "Dark" }));

    expect(app.activeSpaceId).toBe(1);
    expect(loadSpaceAppearance(2).theme_choice).toBe("dark");
  });

  it("starts with Aura AI off for a new space and reveals settings when enabled", async () => {
    app.spaces = [
      { id: 1, name: "Personal", icon: "layers", position: 0, created_at: 0 },
      { id: 2, name: "Work", icon: "layers", position: 1, created_at: 0 }
    ];
    app.activeSpaceId = 1;
    vi.mocked(api.getAiConfig).mockResolvedValue({
      base_url: "",
      model: "",
      embedding_base_url: "",
      embedding_model: "",
      extra_headers: [],
      enabled: false,
      configured: false,
      has_key: false,
      has_embedding_key: false
    });
    render(AiSettings);

    await chooseDropdown("Edit space", "Work");
    const toggle = await screen.findByRole("switch", {
      name: "Enable Aura AI"
    });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    expect(
      screen.queryByRole("button", { name: "OpenAI" })
    ).not.toBeInTheDocument();

    await fireEvent.click(toggle);

    await waitFor(() =>
      expect(
        screen.getByRole("switch", { name: "Enable Aura AI" })
      ).toHaveAttribute("aria-checked", "true")
    );
    expect(
      await screen.findByRole("button", { name: "OpenAI" })
    ).toBeInTheDocument();
    expect(api.setAiEnabled).toHaveBeenCalledWith(true, 2);
  });

  it("opens the requested account from a reconnect request", async () => {
    app.accounts = [googleAccount()];
    render(Settings, {
      props: {
        onClose: vi.fn(),
        initialTab: "account",
        focusAccountId: app.accounts[0].id,
        requestNonce: 1
      }
    });
    await tick();
    expect(screen.getByRole("tab", { name: "Account" })).toHaveAttribute(
      "aria-selected",
      "true"
    );
    await fireEvent.click(screen.getByRole("button", { name: "Account setup" }));
    expect(screen.getByRole("button", { name: "Reconnect Google" }))
      .toBeInTheDocument();
  });
});
