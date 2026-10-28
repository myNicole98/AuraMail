import type { Base16Key } from "./theme/base16";
import type { ThemeChoice } from "./theme/catalog";
import type { ThemeBackground, ThemePanes } from "./settings";

export type { LayoutState } from "./stores/layout";

export type ThemePalettePayload = { scheme: string } & Record<Base16Key, string>;

export type ThemeSettings = {
  choice: ThemeChoice;
  accent: string | null;
  background: ThemeBackground;
  panes: ThemePanes;
};

export type SignatureSocial = {
  label: string;
  url: string;
};

export type SignatureFields = {
  profession: string;
  phone: string;
  location: string;
  website: string;
  socials: SignatureSocial[];
};

export type Space = {
  id: number;
  name: string;
  icon: string;
  position: number;
  created_at: number;
};

export type SpaceAppearance = {
  theme_choice: ThemeChoice;
  theme_accent: string;
  theme_background: ThemeBackground;
  theme_panes: ThemePanes;
  layout_mode: "classic" | "reader" | "stacked";
  message_preview: "compact" | "preview";
  dark_email: boolean;
};

export type SpaceGeneral = {
  time_format: "system" | "12h" | "24h";
  message_order: "newest" | "oldest";
  mark_read_delay: number;
  attachment_thumbnails: boolean;
  reveal_download: boolean;
  always_load_images: boolean;
  profile_pictures: boolean;
  trusted_senders: string[];
};

export type SpaceUnread = {
  space_id: number;
  unread: number;
};

export type SpaceWorkflow = {
  selectedFolderId: number | null;
  selectedTabId: number | null;
  selectedMessageId: number | null;
  unified: "inbox" | "sent" | "archive" | "spam" | "trash" | "starred";
  draftsOpen: boolean;
};

export type Account = {
  id: number;
  display_name: string;
  email: string;
  username: string;
  imap_host: string;
  imap_port: number;
  imap_security: string;
  smtp_host: string;
  smtp_port: number;
  smtp_security: string;
  color: string;
  signature: string;
  signature_fields: SignatureFields;
  signature_on_new: boolean;
  auth_type: "password" | "oauth" | "proton";
  oauth_provider: string;
  space_id: number;
  position: number;
  sync_interval_secs: number;
  created_at: number;
};

export type CalendarAccount = {
  id: number;
  displayName: string;
  baseUrl: string;
  username: string;
  spaceId: number;
  position: number;
  createdAt: number;
};

export type CalendarAccountInput = {
  displayName: string;
  baseUrl: string;
  username: string;
  password: string;
  spaceId: number;
};

export type CalendarSummary = {
  id: number;
  calendarAccountId: number;
  href: string;
  name: string;
  color: string | null;
  syncToken: string | null;
  lastSyncedAt: number | null;
  readOnly: boolean;
};

export type CalendarEvent = {
  id: number;
  calendarId: number;
  calendarName: string;
  calendarColor: string | null;
  href: string;
  etag: string | null;
  uid: string;
  summary: string;
  description: string;
  location: string;
  startsAt: number;
  endsAt: number | null;
  allDay: boolean;
  readOnly: boolean;
  recurrence: string;
  occurrenceKey?: string;
  seriesStartsAt?: number;
  seriesEndsAt?: number | null;
};

export type CalendarSyncResult = {
  calendars: number;
  events: number;
  errors: string[];
};

export type CalendarEventInput = {
  summary: string;
  description: string;
  location: string;
  startsAt: number;
  endsAt: number | null;
  allDay: boolean;
  recurrence?: string;
};

export type AccountDiscovery = {
  imap_host: string;
  imap_port: number;
  imap_security: string;
  smtp_host: string;
  smtp_port: number;
  smtp_security: string;
  source: string;
};

export type AccountInput = {
  display_name: string;
  email: string;
  username: string;
  password: string;
  imap_host: string;
  imap_port: number;
  imap_security: string;
  smtp_host: string;
  smtp_port: number;
  smtp_security: string;
  color: string;
  signature: string;
  signature_fields: SignatureFields;
  signature_on_new?: boolean;
  space_id: number;
};

export type ExtraHeader = [string, string];

export type AiConfig = {
  base_url: string;
  model: string;
  embedding_base_url: string;
  embedding_model: string;
  extra_headers: ExtraHeader[];
  enabled: boolean;
  configured: boolean;
  has_key: boolean;
  has_embedding_key: boolean;
};

export type AiConfigInput = {
  base_url: string;
  model: string;
  embedding_base_url: string;
  embedding_model: string;
  extra_headers: ExtraHeader[];
  api_key?: string | null;
  embedding_api_key?: string | null;
};

export type SemanticStatus = {
  configured: boolean;
  model: string;
  indexed: number;
  pending: number;
  indexing: boolean;
  error: string | null;
};

export type DiscoveredProvider = {
  id: string;
  base_url: string;
  models: string[];
};

export type AiChatContext = "thread" | "email" | "inbox" | "none";

export type ChatMessage = {
  role: "user" | "assistant";
  content: string;
};

export type AiChatRequest = {
  context: AiChatContext;
  messageId?: number;
  question: string;
  history: ChatMessage[];
  fullBody: boolean;
};

export type AiDraft = {
  text: string;
};

export type AiAuraDraft = {
  tone: "direct" | "warm" | "concise";
  text: string;
};

export type AiAuraRail = {
  summary: string;
  ask: string;
  deadline: string | null;
  tone: string | null;
  question: string | null;
  options: string[];
  drafts: AiAuraDraft[];
  messageCount: number;
};

export type AiAuraAskResponse = {
  answer: string;
  drafts: AiAuraDraft[];
};

export type AiPreferences = {
  auto_classify: boolean;
  auto_rail: boolean;
  full_body_context: boolean;
  match_voice: boolean;
};

export type AiVoiceProfile = {
  text: string;
  updatedAt: number;
  sampleCount: number;
};

export type MailPreferences = {
  trusted_senders: string[];
  always_load_images: boolean;
  profile_pictures: boolean;
};

export type DesktopCloseAction = "tray" | "quit";
export type DesktopMinimizeAction = "tray" | "taskbar";

export type DesktopPreferences = {
  notifications: boolean;
  newMailSound: boolean;
  sentMailSound: boolean;
  closeAction: DesktopCloseAction;
  minimizeAction: DesktopMinimizeAction;
};

export type Tab = {
  id: number;
  space_id: number;
  name: string;
  description: string;
  position: number;
  removable: boolean;
  primary: boolean;
  color: string | null;
  icon: string | null;
};

export type MessageSnippet = {
  message_id: number;
  snippet: string;
};

export type TabUnreadCount = {
  tab_id: number;
  unread: number;
};

export type TabUnreadCounts = {
  all: number;
  tabs: TabUnreadCount[];
};

export type Classification = {
  tab_id: number | null;
  confidence: number;
  model: string;
};

export type ClassificationOutcome = {
  message_id: number;
  classification: Classification | null;
  error: string | null;
  error_kind: string | null;
};

export type ClassificationResult = {
  message_id: number;
  classification: Classification | null;
  error: string | null;
};

export type ClassificationDone = {
  accountId: number;
  classified: number;
  failed: number;
  results: ClassificationResult[];
};

export type ClassificationProgress = {
  total: number;
  done: number;
  aborting: boolean;
  abortable: boolean;
};

export type AiChatResponse = {
  answer: string;
};

export type Folder = {
  id: number;
  account_id: number;
  path: string;
  name: string;
  role: "inbox" | "sent" | "drafts" | "trash" | "archive" | "all" | "junk" | "other";
  delimiter: string;
  uid_validity: number;
  highest_uid: number;
  unread_count: number;
};

export type Message = {
  id: number;
  account_id: number;
  folder_id: number;
  uid: number;
  message_id: string | null;
  references: string | null;
  thread_key: string;
  from_addr: string;
  from_name: string;
  to_addrs: string[];
  cc_addrs: string[];
  subject: string;
  date: number;
  snippet: string;
  is_seen: boolean;
  is_starred: boolean;
  has_attachments: boolean;
  body_cached: boolean;
  tab_id: number | null;
};

export type ThreadMessage = Message & {
  thread_count: number;
  thread_unread_count: number;
  thread_has_inbox: boolean;
};

export type Attachment = {
  id: number;
  message_id: number;
  filename: string;
  mime_type: string;
  size: number;
  content_id: string | null;
};

export type AttachmentPreview = {
  mime_type: string;
  data_url: string;
};

export type MessageDetail = {
  message: Message;
  text_body: string | null;
  html_body: string | null;
  image_html: string | null;
  attachments: Attachment[];
};

export type MessageOrder = "newest" | "oldest";

export type MessageQuery = {
  folder_id?: number;
  account_id?: number;
  space_id?: number;
  tab_id?: number | null;
  unified?: "inbox" | "sent" | "archive" | "spam" | "trash" | "starred";
  search?: string;
  limit?: number;
  offset?: number;
  order?: MessageOrder;
};

export type SyncState = "syncing" | "idle" | "offline" | "reauth" | "error";

export type OAuthStatus = {
  configured: boolean;
  reason: string | null;
};

export type GoogleOAuthStatus = OAuthStatus;
export type MicrosoftOAuthStatus = OAuthStatus;

export type OAuthUiStatus = {
  phase: "waiting" | "connecting";
  url?: string | null;
};

export type ProtonAuthStart =
  | { mode: "add"; spaceId: number; username: string; password: string }
  | { mode: "reconnect"; accountId: number; username: string; password: string };

export type ProtonAuthResponse =
  | { type: "totp"; value: string }
  | { type: "mailboxPassword"; value: string }
  | { type: "verificationDone" }
  | { type: "captchaToken"; value: string };

export type ProtonAccessLimited = {
  status: "accessLimited";
  message: string;
  appealUrl: string;
};

export type ProtonAccessLimitedEvent = {
  username: string;
  message: string;
  appealUrl: string;
};

export type ProtonAuthStatus =
  | { status: "needsTotp"; loginId: string; error: string | null }
  | { status: "needsMailboxPassword"; loginId: string; error: string | null }
  | {
      status: "needsVerification";
      loginId: string;
      url: string;
      methods: string[];
      error: string | null;
    }
  | ProtonAccessLimited
  | { status: "complete"; account: Account };

export type SyncFolderStatus = {
  accountId: number;
  folderId: number;
  syncing: boolean;
};

export type SyncStatus = {
  accountId: number;
  state: SyncState;
  message?: string;
};

export type SyncNew = {
  accountId: number;
  folderId: number;
  count: number;
};

export type ComposeMode = "new" | "reply" | "reply-all" | "forward";

export type AttachmentUpload = {
  name: string;
  mimeType: string;
  data: string;
};

export type InlineImageUpload = {
  name: string;
  mimeType: string;
  data: string;
  contentId: string;
};

export type Contact = {
  name: string;
  email: string;
};

export type AiComposeRequest = {
  mode: "rewrite" | "generate";
  instruction: string;
  text: string;
  subject: string;
};

export type ComposeInput = {
  accountId: number;
  to: string[];
  cc: string[];
  subject: string;
  body: string;
  htmlBody: string | null;
  inlineImages: InlineImageUpload[];
  inReplyTo: string | null;
  references: string | null;
  attachments: AttachmentUpload[];
};

export type Draft = {
  id: number;
  accountId: number;
  toAddrs: string[];
  ccAddrs: string[];
  subject: string;
  body: string;
  bodyHtml: string | null;
  inlineImages: InlineImageUpload[];
  inReplyTo: string | null;
  references: string | null;
  attachments: AttachmentUpload[];
  updatedAt: number;
};

export type DraftSummary = {
  id: number;
  accountId: number;
  toAddrs: string[];
  ccAddrs: string[];
  subject: string;
  snippet: string;
  hasAttachments: boolean;
  updatedAt: number;
};
