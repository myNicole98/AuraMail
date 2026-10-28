import { invoke } from "@tauri-apps/api/core";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import type {
  Account,
  AccountDiscovery,
  AccountInput,
  AttachmentPreview,
  CalendarAccount,
  CalendarAccountInput,
  CalendarEvent,
  CalendarEventInput,
  CalendarSummary,
  CalendarSyncResult,
  AiAuraAskResponse,
  AiAuraRail,
  AiChatRequest,
  AiChatResponse,
  AiConfig,
  AiConfigInput,
  AiDraft,
  AiPreferences,
  AiVoiceProfile,
  SemanticStatus,
  Classification,
  AiComposeRequest,
  ChatMessage,
  ClassificationOutcome,
  ComposeInput,
  Contact,
  DesktopPreferences,
  DiscoveredProvider,
  Draft,
  DraftSummary,
  Folder,
  GoogleOAuthStatus,
  MailPreferences,
  MicrosoftOAuthStatus,
  Message,
  MessageDetail,
  MessageQuery,
  MessageSnippet,
  OAuthUiStatus,
  ProtonAuthResponse,
  ProtonAuthStart,
  ProtonAuthStatus,
  Space,
  SpaceUnread,
  Tab,
  TabUnreadCounts,
  ThreadMessage,
  ThemePalettePayload,
  ThemeSettings
} from "./types";
import { activeSpaceId } from "./spaces";

function currentSpaceId(): number {
  return activeSpaceId() ?? 0;
}

export const api = {
  listAccounts: () => invoke<Account[]>("list_accounts"),
  listSpaces: () => invoke<Space[]>("list_spaces"),
  spaceUnreadCounts: () => invoke<SpaceUnread[]>("space_unread_counts"),
  createSpace: (name: string, icon = "layers") =>
    invoke<Space>("create_space", { name, icon }),
  renameSpace: (id: number, name: string, icon?: string) =>
    invoke<Space>("rename_space", { id, name, icon }),
  deleteSpace: (
    id: number,
    action:
      | { kind: "delete_accounts" }
      | { kind: "migrate"; targetSpaceId: number }
  ) => invoke<Space[]>("delete_space", { id, action }),
  reorderAccounts: (spaceId: number, ids: number[]) =>
    invoke<Account[]>("reorder_accounts", { spaceId, ids }),
  moveAccount: (id: number, spaceId: number) =>
    invoke<Account>("move_account", { id, spaceId }),
  addAccount: (input: AccountInput) => invoke<Account>("add_account", { input }),
  updateAccount: (id: number, input: AccountInput) =>
    invoke<Account>("update_account", { id, input }),
  setAccountColor: (id: number, color: string) =>
    invoke<Account>("set_account_color", { id, color }),
  discoverAccountSettings: (username: string) =>
    invoke<AccountDiscovery>("discover_account_settings", { username }),
  deleteAccount: (id: number) => invoke<void>("delete_account", { id }),
  googleOAuthStatus: () => invoke<GoogleOAuthStatus>("google_oauth_status"),
  startGoogleOAuth: (spaceId = currentSpaceId()) =>
    invoke<Account>("start_google_oauth", { spaceId }),
  reconnectGoogleAccount: (id: number) =>
    invoke<Account>("reconnect_google_account", { id }),
  cancelGoogleOAuth: () => invoke<void>("cancel_google_oauth"),
  microsoftOAuthStatus: () => invoke<MicrosoftOAuthStatus>("microsoft_oauth_status"),
  startMicrosoftOAuth: (spaceId = currentSpaceId()) =>
    invoke<Account>("start_microsoft_oauth", { spaceId }),
  reconnectMicrosoftAccount: (id: number) =>
    invoke<Account>("reconnect_microsoft_account", { id }),
  cancelMicrosoftOAuth: () => invoke<void>("cancel_microsoft_oauth"),
  startProtonLogin: (request: ProtonAuthStart) =>
    invoke<ProtonAuthStatus>("start_proton_login", { request }),
  continueProtonLogin: (loginId: string, response: ProtonAuthResponse) =>
    invoke<ProtonAuthStatus>("continue_proton_login", { loginId, response }),
  cancelProtonLogin: (loginId: string) =>
    invoke<void>("cancel_proton_login", { loginId }),
  setSyncInterval: (seconds: number) =>
    invoke<Account[]>("set_sync_interval", { seconds }),
  getFastSyncInterval: () => invoke<number>("get_fast_sync_interval"),
  setFastSyncInterval: (seconds: number) =>
    invoke<number>("set_fast_sync_interval", { seconds }),
  getAiConfig: (spaceId = currentSpaceId()) =>
    invoke<AiConfig>("get_ai_config", { spaceId }),
  setAiEnabled: (enabled: boolean, spaceId = currentSpaceId()) =>
    invoke<AiConfig>("set_ai_enabled", { enabled, spaceId }),
  aiHasKey: (baseUrl: string, spaceId = currentSpaceId()) =>
    invoke<boolean>("ai_has_key", { baseUrl, spaceId }),
  discoverAiProviders: () => invoke<DiscoveredProvider[]>("discover_ai_providers"),
  listAiModels: (baseUrl: string, apiKey?: string, spaceId = currentSpaceId()) =>
    invoke<string[]>("list_ai_models", { baseUrl, apiKey, spaceId }),
  getAiPreferences: (spaceId = currentSpaceId()) =>
    invoke<AiPreferences>("get_ai_preferences", { spaceId }),
  setAiPreferences: (preferences: AiPreferences, spaceId = currentSpaceId()) =>
    invoke<AiPreferences>("set_ai_preferences", { preferences, spaceId }),
  getAiVoiceProfile: (spaceId = currentSpaceId()) =>
    invoke<AiVoiceProfile | null>("get_ai_voice_profile", { spaceId }),
  learnAiVoice: (spaceId = currentSpaceId()) =>
    invoke<AiVoiceProfile>("learn_ai_voice", { spaceId }),
  clearAiVoiceProfile: (spaceId = currentSpaceId()) =>
    invoke<void>("clear_ai_voice_profile", { spaceId }),
  getSemanticStatus: (spaceId = currentSpaceId()) =>
    invoke<SemanticStatus>("get_semantic_status", { spaceId }),
  startSemanticIndex: (spaceId = currentSpaceId()) =>
    invoke<SemanticStatus>("start_semantic_index", { spaceId }),
  setAiConfig: (input: AiConfigInput, spaceId = currentSpaceId()) =>
    invoke<AiConfig>("set_ai_config", { input, spaceId }),
  testAi: (spaceId = currentSpaceId()) => invoke<string>("test_ai", { spaceId }),
  aiStatus: (spaceId = currentSpaceId()) =>
    invoke<boolean>("ai_status", { spaceId }),
  aiReply: (messageId: number, guidance?: string | null, fullBody = false) =>
    invoke<AiDraft>("ai_reply", { messageId, guidance, fullBody }),
  aiAuraRail: (messageId: number, answer?: string | null, fullBody = false) =>
    invoke<AiAuraRail>("ai_aura_rail", { messageId, answer, fullBody }),
  aiAuraAsk: (
    messageId: number,
    question: string,
    history: ChatMessage[],
    existing: string[],
    fullBody = false
  ) =>
    invoke<AiAuraAskResponse>("ai_aura_ask", {
      messageId,
      question,
      history,
      existing,
      fullBody
    }),
  aiChat: (request: AiChatRequest, spaceId = currentSpaceId()) =>
    invoke<AiChatResponse>("ai_chat", { request, spaceId }),
  aiCompose: (request: AiComposeRequest, spaceId = currentSpaceId()) =>
    invoke<AiDraft>("ai_compose", { request, spaceId }),
  listContacts: (query?: string, limit?: number) =>
    invoke<Contact[]>("list_contacts", { query, limit }),
  openExternal: (url: string) => invoke<void>("open_external", { url }),
  openComposeWindow: (mode: string, messageId?: number | null, draftId?: number | null) =>
    invoke<void>("open_compose_window", {
      mode,
      messageId: messageId ?? null,
      draftId: draftId ?? null
    }),
  syncAccount: (accountId: number) => invoke<void>("sync_account", { accountId }),
  listFolders: (accountId?: number, spaceId = currentSpaceId()) =>
    invoke<Folder[]>("list_folders", { accountId, spaceId }),
  listMessages: (params: MessageQuery) => invoke<Message[]>("list_messages", params),
  listThreads: (params: MessageQuery) =>
    invoke<ThreadMessage[]>("list_threads", params),
  listThreadMessages: (accountId: number, threadKey: string) =>
    invoke<Message[]>("list_thread_messages", { accountId, threadKey }),
  listTabs: (spaceId = currentSpaceId()) =>
    invoke<Tab[]>("list_tabs", { spaceId }),
  listCalendars: (spaceId = currentSpaceId()) =>
    invoke<CalendarSummary[]>("list_calendars", { spaceId }),
  listCalendarEvents: (
    from: number,
    to: number,
    spaceId = currentSpaceId(),
    limit = 200
  ) => invoke<CalendarEvent[]>("list_calendar_events", { spaceId, from, to, limit }),
  listCalendarAccounts: (spaceId = currentSpaceId()) =>
    invoke<CalendarAccount[]>("list_calendar_accounts", { spaceId }),
  reorderCalendarAccounts: (spaceId: number, ids: number[]) =>
    invoke<CalendarAccount[]>("reorder_calendar_accounts", { spaceId, ids }),
  moveCalendarAccount: (accountId: number, spaceId: number) =>
    invoke<CalendarAccount>("move_calendar_account", { accountId, spaceId }),
  addCalendarAccount: (input: CalendarAccountInput) =>
    invoke<CalendarAccount>("add_calendar_account", { input }),
  removeCalendarAccount: (accountId: number) =>
    invoke<void>("remove_calendar_account", { accountId }),
  unsubscribeCalendar: (calendarId: number) =>
    invoke<void>("unsubscribe_calendar", { calendarId }),
  syncCalendar: (accountId: number) =>
    invoke<CalendarSyncResult>("sync_calendar", { accountId }),
  createCalendarEvent: (calendarId: number, input: CalendarEventInput) =>
    invoke<CalendarSyncResult>("create_calendar_event", { calendarId, input }),
  updateCalendarEvent: (eventId: number, input: CalendarEventInput) =>
    invoke<CalendarSyncResult>("update_calendar_event", { eventId, input }),
  deleteCalendarEvent: (eventId: number) =>
    invoke<CalendarSyncResult>("delete_calendar_event", { eventId }),
  tabUnreadCounts: (
    params: Pick<MessageQuery, "folder_id" | "account_id" | "space_id" | "unified">
  ) => invoke<TabUnreadCounts>("tab_unread_counts", params),
  createTab: (
    name: string,
    description: string,
    color?: string | null,
    icon?: string | null,
    spaceId = currentSpaceId()
  ) => invoke<Tab>("create_tab", { name, description, color, icon, spaceId }),
  updateTab: (
    id: number,
    name: string,
    description: string,
    color?: string | null,
    icon?: string | null,
    spaceId = currentSpaceId()
  ) =>
    invoke<Tab>("update_tab", { id, name, description, color, icon, spaceId }),
  deleteTab: (id: number, spaceId = currentSpaceId()) =>
    invoke<void>("delete_tab", { id, spaceId }),
  reorderTabs: (ids: number[], spaceId = currentSpaceId()) =>
    invoke<Tab[]>("reorder_tabs", { ids, spaceId }),
  classifyMessage: (messageId: number, reclassify = false) =>
    invoke<Classification>("classify_message", { messageId, reclassify }),
  classifyMessages: (ids: number[]) =>
    invoke<ClassificationOutcome[]>("classify_messages", { ids }),
  classifyUnclassified: (
    accountId?: number,
    folderId?: number,
    spaceId = currentSpaceId()
  ) =>
    invoke<[number, number]>("classify_unclassified", {
      accountId,
      folderId,
      spaceId
    }),
  getMessage: (messageId: number) => invoke<MessageDetail>("get_message", { messageId }),
  fillMessageSnippets: (messageIds: number[]) =>
    invoke<MessageSnippet[]>("fill_message_snippets", { messageIds }),
  getMailPreferences: (spaceId = currentSpaceId()) =>
    invoke<MailPreferences>("get_mail_preferences", { spaceId }),
  setMailPreferences: (
    preferences: MailPreferences,
    spaceId = currentSpaceId()
  ) => invoke<MailPreferences>("set_mail_preferences", { preferences, spaceId }),
  getSenderAvatar: (email: string, spaceId = currentSpaceId()) =>
    invoke<string | null>("get_sender_avatar", { email, spaceId }),
  getDesktopPreferences: () =>
    invoke<DesktopPreferences>("get_desktop_preferences"),
  setDesktopPreferences: (preferences: DesktopPreferences) =>
    invoke<DesktopPreferences>("set_desktop_preferences", { preferences }),
  refreshTrayUnread: () => invoke<void>("refresh_tray_unread"),
  markRead: (messageId: number, read: boolean) =>
    invoke<Message>("mark_read", { messageId, read }),
  moveMessageToFolder: (messageId: number, folderId: number) =>
    invoke<void>("move_message_to_folder", { messageId, folderId }),
  setMessageClassification: (messageId: number, tabId: number | null) =>
    invoke<Classification>("set_message_classification", { messageId, tabId }),
  toggleStar: (messageId: number) => invoke<Message>("toggle_star", { messageId }),
  trashMessage: (messageId: number) => invoke<void>("trash_message", { messageId }),
  archiveMessage: (messageId: number) => invoke<void>("archive_message", { messageId }),
  archiveThread: (accountId: number, threadKey: string) =>
    invoke<void>("archive_thread", { accountId, threadKey }),
  deleteMessage: (messageId: number) => invoke<void>("delete_message", { messageId }),
  downloadAttachment: (attachmentId: number) =>
    invoke<string>("download_attachment", { attachmentId }),
  openDownload: (path: string) => openPath(path),
  revealDownload: (path: string) => revealItemInDir(path),
  attachmentPreview: (attachmentId: number) =>
    invoke<AttachmentPreview | null>("attachment_preview", { attachmentId }),
  loadRemoteImages: (messageId: number) =>
    invoke<string>("load_remote_images", { messageId }),
  sendMessage: (input: ComposeInput, draftId?: number) =>
    invoke<void>("send_message", { input, draftId }),
  saveDraft: (input: ComposeInput, draftId?: number) =>
    invoke<Draft>("save_draft", { input, draftId }),
  listDrafts: (accountId?: number, spaceId = currentSpaceId()) =>
    invoke<DraftSummary[]>("list_drafts", { accountId, spaceId }),
  getDraft: (draftId: number) => invoke<Draft>("get_draft", { draftId }),
  deleteDraft: (draftId: number) => invoke<void>("delete_draft", { draftId }),
  getThemeSettings: (spaceId = currentSpaceId()) =>
    invoke<ThemeSettings>("get_theme_settings", { spaceId }),
  setThemeSettings: (
    settings: ThemeSettings,
    spaceId = currentSpaceId()
  ) => invoke<ThemeSettings>("set_theme_settings", { settings, spaceId }),
  setActiveThemeSpace: (spaceId = currentSpaceId()) =>
    invoke<ThemeSettings>("set_active_theme_space", { spaceId }),
  reloadTheme: () => invoke<ThemePalettePayload | null>("reload_theme")
};
