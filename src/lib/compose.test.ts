import { describe, expect, it } from "vitest";
import type { Account, Message } from "./types";
import {
  appendDraft,
  base64ByteLength,
  composeFocusTarget,
  contactMatches,
  htmlToPlainText,
  resolveSignature,
  SIGNATURE_PRESETS,
  signatureHtml,
  sanitizeSignatureImages,
  signatureTemplateHtml,
  recipientToken,
  replaceRecipientToken,
  sanitizeComposeHtml,
  textToHtml,
  isEmailRecipient,
  readFileAsBase64,
  recipientProblem,
  replyAllRecipients,
  replyDraftInput,
  replyReferences,
  splitRecipients
} from "./compose";

describe("Aura draft handoff", () => {
  const message: Message = {
    id: 1,
    account_id: 1,
    folder_id: 1,
    uid: 1,
    message_id: "<one@example.com>",
    references: "<zero@example.com>",
    thread_key: "thread-1",
    from_addr: "ada@example.com",
    from_name: "Ada",
    to_addrs: ["me@example.com", "alex@example.com"],
    cc_addrs: ["cc@example.com"],
    subject: "Re: Hello",
    date: 1,
    snippet: "Hello",
    is_seen: true,
    is_starred: false,
    has_attachments: false,
    body_cached: true,
    tab_id: null
  };
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

  it("builds a reply draft with reply-all recipients and thread headers", () => {
    const input = replyDraftInput(message, [account], "Yes, it applies.");

    expect(input.to).toEqual(["ada@example.com", "alex@example.com"]);
    expect(input.cc).toEqual(["cc@example.com"]);
    expect(input.subject).toBe("Re: Hello");
    expect(input.body).toBe("Yes, it applies.");
    expect(input.htmlBody).toBeNull();
    expect(input.inReplyTo).toBe("<one@example.com>");
    expect(input.references).toBe("<zero@example.com> <one@example.com>");
    expect(input.inlineImages).toEqual([]);
    expect(input.attachments).toEqual([]);
  });

  it("adds a single Re prefix only when needed", () => {
    expect(
      replyDraftInput({ ...message, subject: "Hello" }, [account], "Body").subject
    ).toBe("Re: Hello");
  });
});

describe("compose focus target", () => {
  it("starts replies in the body and everything else in the recipient field", () => {
    expect(composeFocusTarget("reply")).toBe("body");
    expect(composeFocusTarget("reply-all")).toBe("body");
    expect(composeFocusTarget("new")).toBe("to");
    expect(composeFocusTarget("forward")).toBe("to");
  });
});

describe("recipient parsing", () => {
  it("accepts bare and display-name mailboxes", () => {
    expect(isEmailRecipient("ada@example.com")).toBe(true);
    expect(isEmailRecipient("Ada Lovelace <ada@example.com>")).toBe(true);
    expect(isEmailRecipient('"Doe, Ada" <ada@example.com>')).toBe(true);
  });

  it("rejects malformed and multiple-at addresses", () => {
    expect(isEmailRecipient("not-an-email")).toBe(false);
    expect(isEmailRecipient("a@b@example.com")).toBe(false);
    expect(isEmailRecipient("Ada <ada@example.com")).toBe(false);
  });

  it("splits display-name mailboxes with quoted commas", () => {
    expect(splitRecipients('"Doe, Ada" <ada@example.com>, grace@example.com')).toEqual([
      '"Doe, Ada" <ada@example.com>',
      "grace@example.com"
    ]);
  });

  it("reports an empty recipient list separately from invalid addresses", () => {
    expect(recipientProblem("", "")).toBe("Add at least one recipient.");
    expect(recipientProblem("not-an-email", "")).toBe(
      "Check the recipient address: not-an-email"
    );
    expect(recipientProblem("", "team@example.com")).toBeNull();
  });
});

describe("replyAllRecipients", () => {
  it("keeps original To and Cc structure", () => {
    const recipients = replyAllRecipients(
      {
        from_addr: "ada@example.com",
        to_addrs: ["me@example.com", "grace@example.com"],
        cc_addrs: ["team@example.com"]
      },
      ["me@example.com"]
    );

    expect(recipients.to).toEqual(["ada@example.com", "grace@example.com"]);
    expect(recipients.cc).toEqual(["team@example.com"]);
  });

  it("removes own addresses and duplicates", () => {
    const recipients = replyAllRecipients(
      {
        from_addr: "ada@example.com",
        to_addrs: ["me@example.com", "ada@example.com"],
        cc_addrs: ["me@example.com", "team@example.com"]
      },
      ["Me@Example.com"]
    );

    expect(recipients.to).toEqual(["ada@example.com"]);
    expect(recipients.cc).toEqual(["team@example.com"]);
  });
});

describe("replyReferences", () => {
  it("appends the parent to an existing multi-message chain", () => {
    expect(
      replyReferences("<3@example.com>", "<1@example.com> <2@example.com>")
    ).toBe("<1@example.com> <2@example.com> <3@example.com>");
  });

  it("uses only the parent when no chain exists and avoids duplicates", () => {
    expect(replyReferences("<2@example.com>", null)).toBe("<2@example.com>");
    expect(replyReferences("<2@example.com>", "<1@example.com> <2@example.com>")).toBe(
      "<1@example.com> <2@example.com>"
    );
    expect(replyReferences(null, null)).toBeNull();
  });
});

describe("base64ByteLength", () => {
  it("computes decoded sizes from padded base64", () => {
    expect(base64ByteLength("")).toBe(0);
    expect(base64ByteLength("aGk=")).toBe(2);
    expect(base64ByteLength("aGVsbG8=")).toBe(5);
    expect(base64ByteLength("cGF5bG9hZA==")).toBe(7);
  });
});

describe("readFileAsBase64", () => {
  it("returns the bare base64 payload without the data URL prefix", async () => {
    const file = new File([new Uint8Array([104, 105])], "a.txt", {
      type: "text/plain"
    });
    await expect(readFileAsBase64(file)).resolves.toBe("aGk=");
  });
});

describe("appendDraft", () => {
  it("inserts a draft into an empty body unchanged", () => {
    expect(appendDraft("", "Hello there")).toBe("Hello there");
  });

  it("separates an existing body from the draft with one blank line", () => {
    expect(appendDraft("My own words\n", "AI draft")).toBe("My own words\n\nAI draft");
    expect(appendDraft("   ", "AI draft")).toBe("AI draft");
  });

  it("ignores blank drafts", () => {
    expect(appendDraft("Existing", "  \n ")).toBe("Existing");
  });
});

describe("compose HTML", () => {
  it("converts plain text to escaped HTML paragraphs", () => {
    expect(textToHtml("Hello <Ada>\n\nSecond line")).toBe(
      "Hello &lt;Ada&gt;<br><br>Second line"
    );
  });

  it("extracts plain text from sanitized HTML", () => {
    expect(htmlToPlainText("<p>Hello</p><p>Second</p>")).toBe("Hello\nSecond");
  });

  it("removes scripts, event handlers and javascript links", () => {
    const sanitized = sanitizeComposeHtml(
      '<p onclick="evil()">Hi <a href="javascript:evil()">link</a></p><script>evil()</script>'
    );
    expect(sanitized).not.toContain("script");
    expect(sanitized).not.toContain("onclick");
    expect(sanitized).not.toContain("javascript:");
    expect(sanitized).toContain("Hi");
  });

  it("sanitizes pasted HTML but keeps safe formatting and inline image metadata", () => {
    const sanitized = sanitizeComposeHtml(
      '<b>Bold</b><img src="data:image/png;base64,abc" data-inline-id="one" onerror="evil()">'
    );
    expect(sanitized).toContain("<b>Bold</b>");
    expect(sanitized).toContain('data-inline-id="one"');
    expect(sanitized).not.toContain("onerror");
  });
});

describe("recipient autocomplete", () => {
  const contacts = [
    { name: "Ada Lovelace", email: "ada@example.com" },
    { name: "Grace Hopper", email: "grace@example.com" },
    { name: "Team", email: "team@example.com" }
  ];

  it("uses the token after the last comma", () => {
    expect(recipientToken("ada@example.com, gra")).toBe("gra");
    expect(recipientToken("")).toBe("");
  });

  it("matches names and addresses case-insensitively", () => {
    expect(contactMatches(contacts, "ada")).toEqual([contacts[0]]);
    expect(contactMatches(contacts, "EXAMPLE.COM")).toHaveLength(3);
    expect(contactMatches(contacts, "zzz")).toEqual([]);
  });

  it("replaces only the current token and keeps earlier recipients", () => {
    expect(
      replaceRecipientToken("ada@example.com, gra", {
        name: "Grace Hopper",
        email: "grace@example.com"
      })
    ).toBe("ada@example.com, Grace Hopper <grace@example.com>");
  });
});

describe("signatures", () => {
  const account = { display_name: "Nicole Frega", email: "nicole@example.com" };

  it("resolves name, first name and email placeholders", () => {
    expect(
      resolveSignature("Best regards,\n{name}\n{first_name}\n{email}", account)
    ).toBe("Best regards,\nNicole Frega\nNicole\nnicole@example.com");
  });

  it("renders an escaped signature block", () => {
    const html = signatureHtml("— {name} <{email}>", account);
    expect(html).toContain('data-auramail-signature="true"');
    expect(html).toContain("Nicole Frega");
    expect(html).toContain("&lt;nicole@example.com&gt;");
  });

  it("renders modular fields, conditional blocks and social badges", () => {
    const signature = signatureHtml(
      '<div>{name}{{#profession}} · {profession}{{/profession}}{{#phone}} · {phone_link}{{/phone}}{{#socials}}<div>{socials}</div>{{/socials}}</div>',
      {
        display_name: "Nicole Frega",
        email: "nicole@example.com",
        signature_fields: {
          profession: "Designer",
          phone: "+39 333 123 4567",
          location: "Milan",
          website: "example.com",
          socials: [
            { label: "LinkedIn", url: "https://linkedin.com/in/nicole" }
          ]
        }
      }
    );
    expect(signature).toContain("Nicole Frega · Designer");
    expect(signature).toContain("tel:+39 333 123 4567");
    expect(signature).toContain("https://linkedin.com/in/nicole");
    expect(signature).toContain("data:image/svg+xml;base64,");
    expect(signature).toContain("alt=\"LinkedIn\"");
    expect(signature).toContain("<img");
    expect(signature).not.toContain("{#profession}");
  });

  it("drops conditional blocks for empty fields", () => {
    const signature = signatureHtml(
      '<div>{name}{{#profession}} · {profession}{{/profession}}</div>',
      { display_name: "Nicole", email: "nicole@example.com" }
    );
    expect(signature).toContain("<div>Nicole</div>");
    expect(signature).not.toContain("·");
  });

  it("ships presets that expose every field", () => {
    expect(SIGNATURE_PRESETS.length).toBe(10);
    for (const preset of SIGNATURE_PRESETS) {
      for (const placeholders of [
        ["{name}"],
        ["{email}", "{email_link}"],
        ["{phone}", "{phone_link}"],
        ["{location}"],
        ["{website}", "{website_link}"],
        ["{socials}"]
      ]) {
        expect(placeholders.some((value) => preset.template.includes(value))).toBe(
          true
        );
      }
    }
  });
});

describe("rich signatures", () => {
  const account = { display_name: "Nicole Frega", email: "nicole@example.com" };

  it("keeps HTML templates and resolves placeholders inside them", () => {
    const html = signatureHtml(
      '<div style="color:#4f46e5"><strong>{name}</strong><br>{email}</div>',
      account
    );
    expect(html).toContain("color:#4f46e5");
    expect(html).toContain("<strong>Nicole Frega</strong>");
    expect(html).toContain("nicole@example.com");
  });

  it("normalizes legacy plain-text signatures to HTML", () => {
    expect(signatureTemplateHtml("Best,\nNicole")).toBe("Best,<br>Nicole");
    expect(signatureTemplateHtml("<b>Best</b>")).toBe("<b>Best</b>");
  });

  it("drops images the main webview cannot load", () => {
    expect(
      sanitizeSignatureImages(
        '<p><img src="https://example.com/a.png"><img src="cid:logo"><img src="data:image/png;base64,AA"></p>'
      )
    ).toBe('<p><img src="data:image/png;base64,AA"></p>');
  });
});
