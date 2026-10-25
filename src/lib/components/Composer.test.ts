import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import Composer from "./Composer.svelte";
import type { Account, Draft, Message, MessageDetail } from "../types";

vi.mock("../api", () => ({
  api: {
    listContacts: vi.fn().mockResolvedValue([])
  }
}));

const account: Account = {
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
  color: "accent",
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
  sync_interval_secs: 300,
  created_at: 0
};

const message: Message = {
  id: 1,
  account_id: 1,
  folder_id: 1,
  uid: 1,
  message_id: "<one@example.com>",
  references: null,
  thread_key: "one@example.com",
  from_addr: "ada@example.com",
  from_name: "Ada",
  to_addrs: ["me@example.com"],
  cc_addrs: [],
  subject: "Hello",
  date: 1_700_000_000,
  snippet: "Hello",
  is_seen: true,
  is_starred: false,
  has_attachments: false,
  body_cached: true,
  tab_id: null
};

const source: MessageDetail = {
  message,
  text_body: "Hello",
  html_body: null,
  image_html: null,
  attachments: []
};

function renderComposer(mode: "new" | "reply") {
  return render(Composer, {
    props: {
      mode,
      source,
      accounts: [account],
      onClose: vi.fn(),
      onSent: vi.fn()
    }
  });
}

afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe("Composer initial focus", () => {
  it("focuses the message body when replying", async () => {
    renderComposer("reply");
    await tick();

    expect(document.activeElement).toBe(screen.getByLabelText("Message body"));
  });

  it("focuses the recipient field for a new message", async () => {
    renderComposer("new");
    await tick();

    expect(document.activeElement).toBe(screen.getByLabelText("To"));
  });

  it("adds the account signature to a draft that has no html body", async () => {
    const draft: Draft = {
      id: 5,
      accountId: 1,
      toAddrs: ["ada@example.com"],
      ccAddrs: [],
      subject: "Re: Hello",
      body: "Yes, it applies.",
      bodyHtml: null,
      inlineImages: [],
      inReplyTo: "<one@example.com>",
      references: "<one@example.com>",
      attachments: [],
      updatedAt: 0
    };
    render(Composer, {
      props: {
        mode: "new",
        source: null,
        draft,
        accounts: [{ ...account, signature: "Ada Lovelace\nDesigner" }],
        onClose: vi.fn(),
        onSent: vi.fn()
      }
    });
    await tick();

    const body = screen.getByLabelText("Message body");
    expect(body.innerHTML).toContain("data-auramail-signature");
    expect(body.textContent).toContain("Ada Lovelace");
    expect(body.textContent).toContain("Yes, it applies.");
  });

  it("only adds a signature on demand when the account setting is off", async () => {
    render(Composer, {
      props: {
        mode: "new",
        source: null,
        accounts: [
          {
            ...account,
            signature: "Ada Lovelace\nDesigner",
            signature_on_new: false
          }
        ],
        onClose: vi.fn(),
        onSent: vi.fn()
      }
    });
    await tick();

    const body = screen.getByLabelText("Message body");
    expect(body.innerHTML).not.toContain("data-auramail-signature");

    await fireEvent.click(screen.getByRole("button", { name: "Add signature" }));

    expect(body.innerHTML).toContain("data-auramail-signature");
    expect(body.textContent).toContain("Ada Lovelace");
  });
});
