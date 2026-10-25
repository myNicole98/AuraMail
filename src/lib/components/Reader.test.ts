import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import { tick } from "svelte";
import Reader from "./Reader.svelte";
import { app } from "../stores/app.svelte";
import type { Account, Message, MessageDetail } from "../types";

vi.mock("../api", () => ({
  api: {
    getSenderAvatar: vi.fn(async () => null),
    loadRemoteImages: vi.fn(async () => ""),
    openExternal: vi.fn(async () => {})
  }
}));

function message(id: number): Message {
  return {
    id,
    account_id: 1,
    folder_id: 1,
    uid: id,
    message_id: `<${id}@example.com>`,
    references: null,
    thread_key: `thread-${id}`,
    from_addr: "ada@example.com",
    from_name: "Ada",
    to_addrs: ["me@example.com"],
    cc_addrs: [],
    subject: `Message ${id}`,
    date: 100,
    snippet: "Preview",
    is_seen: true,
    is_starred: false,
    has_attachments: false,
    body_cached: true,
    tab_id: null
  };
}

function detail(id: number): MessageDetail {
  return {
    message: message(id),
    text_body: null,
    html_body: `<p>Message ${id}</p>`,
    image_html: null,
    attachments: []
  };
}

function account(): Account {
  return {
    id: 1,
    display_name: "Me",
    email: "me@example.com",
    username: "me@example.com",
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
    created_at: 0
  };
}

beforeEach(() => {
  localStorage.clear();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
      unobserve() {}
    }
  );
  app.accounts = [account()];
  app.folders = [];
  app.threadMessages = [];
  app.selectedMessage = detail(1);
  app.selectedMessageId = 1;
  app.messageLoading = false;
  app.attachmentNotice = null;
  app.error = null;
  app.aiConfigured = false;
  app.auraRailOpen = false;
});

afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe("Reader iframe lifecycle", () => {
  it("reuses one message iframe when switching between HTML messages", async () => {
    const { container } = render(Reader);
    await tick();

    const first = container.querySelector('iframe[title="Message body"]');
    expect(first).not.toBeNull();

    app.selectedMessage = detail(2);
    app.selectedMessageId = 2;
    await tick();
    await tick();

    const frames = container.querySelectorAll('iframe[title="Message body"]');
    expect(frames).toHaveLength(1);
    expect(frames[0]).toBe(first);
  });

  it("keeps the iframe mounted while an uncached message loads", async () => {
    const { container } = render(Reader);
    await tick();
    await new Promise((resolve) => setTimeout(resolve, 0));

    const first = container.querySelector('iframe[title="Message body"]');
    expect(first).not.toBeNull();

    app.selectedMessage = null;
    app.selectedMessageId = 2;
    await tick();

    expect(container.querySelectorAll('iframe[title="Message body"]')).toHaveLength(1);
    expect(container.querySelector('iframe[title="Message body"]')).toBe(first);

    app.selectedMessage = detail(2);
    await tick();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(container.querySelectorAll('iframe[title="Message body"]')).toHaveLength(1);
    expect(container.querySelector('iframe[title="Message body"]')).toBe(first);
  });

  it("keeps the same email document when switching messages", async () => {
    const { container } = render(Reader);
    await tick();
    await new Promise((resolve) => setTimeout(resolve, 0));

    const frame = container.querySelector(
      'iframe[title="Message body"]'
    ) as HTMLIFrameElement;
    const documentRef = frame.contentDocument;
    const initialSrcDoc = frame.getAttribute("srcdoc");
    expect(documentRef).not.toBeNull();

    app.selectedMessage = detail(2);
    app.selectedMessageId = 2;
    await tick();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(frame.contentDocument).toBe(documentRef);
    expect(frame.getAttribute("srcdoc")).toBe(initialSrcDoc);
    expect(documentRef?.body.textContent).toContain("Message 2");
  });
});
