import type { Account, ComposeInput, ComposeMode, Message } from "./types";

export type ReplySource = {
  from_addr: string;
  to_addrs: string[];
  cc_addrs: string[];
};

export function composeFocusTarget(mode: ComposeMode): "to" | "body" {
  return mode === "reply" || mode === "reply-all" ? "body" : "to";
}

export function appendDraft(existing: string, incoming: string): string {
  const draft = incoming.trim();
  if (!draft) return existing;
  const body = existing.trim();
  return body ? `${body}\n\n${draft}` : draft;
}

export function splitRecipients(value: string): string[] {
  const parts: string[] = [];
  let current = "";
  let depth = 0;
  let quoted = false;

  for (const character of value) {
    if (character === '"') {
      quoted = !quoted;
    } else if (character === "<" && !quoted) {
      depth += 1;
    } else if (character === ">" && !quoted && depth > 0) {
      depth -= 1;
    }
    if (character === "," && depth === 0 && !quoted) {
      parts.push(current);
      current = "";
      continue;
    }
    current += character;
  }
  parts.push(current);
  return parts.map((part) => part.trim()).filter(Boolean);
}

export function isEmailRecipient(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) return false;
  const bracketed = trimmed.match(/<([^<>]*)>\s*$/);
  const address = (bracketed ? bracketed[1] : trimmed).trim();
  if (!address || /\s/.test(address)) return false;
  const parts = address.split("@");
  if (parts.length !== 2) return false;
  const [local, domain] = parts;
  return Boolean(
    local &&
      domain &&
      !/[<>(),;:]/.test(local) &&
      !/[<>(),;:]/.test(domain)
  );
}

export function recipientProblem(to: string, cc: string): string | null {
  const addresses = [...splitRecipients(to), ...splitRecipients(cc)];
  if (addresses.length === 0) return "Add at least one recipient.";
  const invalid = addresses.find((address) => !isEmailRecipient(address));
  return invalid ? `Check the recipient address: ${invalid}` : null;
}

export function replySubject(subject: string): string {
  const value = subject.trim();
  if (!value) return "";
  return /^re:/i.test(value) ? value : `Re: ${value}`;
}

export function replyDraftInput(
  message: Message,
  accounts: Account[],
  text: string
): ComposeInput {
  const ownAddresses = accounts
    .filter((account) => account.id === message.account_id)
    .flatMap((account) => [account.email, account.username])
    .filter((address): address is string => Boolean(address?.trim()));
  const { to, cc } = replyAllRecipients(message, ownAddresses);
  const trimmedId = message.message_id?.trim() ?? "";
  const inReplyTo = trimmedId
    ? trimmedId.startsWith("<") && trimmedId.endsWith(">")
      ? trimmedId
      : `<${trimmedId}>`
    : null;
  return {
    accountId: message.account_id,
    to,
    cc,
    subject: replySubject(message.subject),
    body: text,
    htmlBody: null,
    inlineImages: [],
    inReplyTo,
    references: replyReferences(inReplyTo, message.references),
    attachments: []
  };
}

export function replyReferences(
  parent: string | null,
  existing: string | null
): string | null {
  const current = existing?.trim() ?? "";
  const id = parent?.trim() ?? "";
  if (!id) return current || null;
  const chain = current.split(/\s+/).filter(Boolean);
  if (!chain.includes(id)) chain.push(id);
  return chain.join(" ");
}

export function base64ByteLength(data: string): number {
  const value = data.replace(/\s/g, "");
  if (!value) return 0;
  const padding = value.endsWith("==") ? 2 : value.endsWith("=") ? 1 : 0;
  return Math.max(0, Math.floor((value.length * 3) / 4) - padding);
}

export function readFileAsBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = typeof reader.result === "string" ? reader.result : "";
      const comma = result.indexOf(",");
      resolve(comma >= 0 ? result.slice(comma + 1) : result);
    };
    reader.onerror = () => reject(reader.error ?? new Error("Could not read file"));
    reader.readAsDataURL(file);
  });
}

export function replyAllRecipients(
  source: ReplySource,
  accountEmails: string[]
): { to: string[]; cc: string[] } {
  const own = new Set(
    accountEmails.map((email) => email.trim().toLowerCase()).filter(Boolean)
  );
  const seen = new Set<string>();
  const to: string[] = [];
  const cc: string[] = [];

  const add = (addresses: string[], target: string[]) => {
    for (const address of addresses) {
      const trimmed = address.trim();
      const key = trimmed.toLowerCase();
      if (!trimmed || own.has(key) || seen.has(key)) continue;
      seen.add(key);
      target.push(trimmed);
    }
  };

  add([source.from_addr, ...source.to_addrs], to);
  add(source.cc_addrs, cc);
  return { to, cc };
}

export type ContactSuggestion = {
  name: string;
  email: string;
};

const BLOCK_TAGS = new Set([
  "ADDRESS",
  "ARTICLE",
  "ASIDE",
  "BLOCKQUOTE",
  "DIV",
  "DL",
  "FIGCAPTION",
  "FIGURE",
  "FOOTER",
  "H1",
  "H2",
  "H3",
  "H4",
  "H5",
  "H6",
  "HEADER",
  "LI",
  "MAIN",
  "NAV",
  "OL",
  "P",
  "PRE",
  "SECTION",
  "TABLE",
  "TR",
  "UL"
]);

const BLOCKED_TAGS = new Set([
  "SCRIPT",
  "STYLE",
  "IFRAME",
  "OBJECT",
  "EMBED",
  "LINK",
  "META",
  "FORM",
  "INPUT",
  "BUTTON",
  "TEXTAREA",
  "SELECT",
  "OPTION",
  "SVG",
  "MATH"
]);

export function textToHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/\r\n/g, "\n")
    .replace(/\n/g, "<br>");
}

function nodeText(node: Node): string {
  if (node.nodeType === Node.TEXT_NODE) return node.nodeValue ?? "";
  if (node.nodeName === "BR") return "\n";
  let value = "";
  node.childNodes.forEach((child) => {
    value += nodeText(child);
  });
  if (BLOCK_TAGS.has(node.nodeName)) value += "\n";
  return value;
}

export function htmlToPlainText(html: string): string {
  const parsed = new DOMParser().parseFromString(html || "", "text/html");
  return nodeText(parsed.body)
    .replace(/\n{3,}/g, "\n\n")
    .replace(/[ \t]+\n/g, "\n")
    .trim();
}

function safeUrl(value: string, allowImageData: boolean): boolean {
  const trimmed = value.trim().toLowerCase();
  if (!trimmed) return false;
  if (trimmed.startsWith("http://") || trimmed.startsWith("https:")) return true;
  if (
    trimmed.startsWith("mailto:") ||
    trimmed.startsWith("tel:") ||
    trimmed.startsWith("cid:")
  ) {
    return true;
  }
  if (allowImageData && trimmed.startsWith("data:image/")) return true;
  return trimmed.startsWith("#");
}

export function sanitizeComposeHtml(html: string): string {
  const parsed = new DOMParser().parseFromString(html || "", "text/html");
  parsed.body.querySelectorAll("*").forEach((element) => {
    if (BLOCKED_TAGS.has(element.tagName)) {
      element.remove();
      return;
    }
    for (const attribute of Array.from(element.attributes)) {
      const name = attribute.name.toLowerCase();
      if (name.startsWith("on") || name === "srcset") {
        element.removeAttribute(attribute.name);
        continue;
      }
      if (name === "href" && !safeUrl(attribute.value, false)) {
        element.removeAttribute(attribute.name);
      }
      if (name === "src" && !safeUrl(attribute.value, true)) {
        element.removeAttribute(attribute.name);
      }
    }
  });
  return parsed.body.innerHTML;
}

export function recipientToken(value: string): string {
  const comma = value.lastIndexOf(",");
  return value.slice(comma + 1).trim();
}

export function contactMatches(
  contacts: ContactSuggestion[],
  query: string,
  limit = 6
): ContactSuggestion[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [];
  return contacts
    .filter(
      (contact) =>
        contact.email.toLowerCase().includes(needle) ||
        contact.name.toLowerCase().includes(needle)
    )
    .slice(0, limit);
}

export function replaceRecipientToken(
  value: string,
  contact: ContactSuggestion
): string {
  const comma = value.lastIndexOf(",");
  const prefix = comma >= 0 ? `${value.slice(0, comma + 1).trimEnd()} ` : "";
  const label = contact.name.trim()
    ? `${contact.name.trim()} <${contact.email.trim()}>`
    : contact.email.trim();
  return `${prefix}${label}`;
}

import type { SignatureFields } from "./types";
import { socialIconSrc, socialNetworkFor } from "./socialIcons";

export type SignatureAccount = {
  display_name: string;
  email: string;
  signature_fields?: SignatureFields;
};

export type SignaturePreset = {
  id: string;
  label: string;
  template: string;
};

export const SIGNATURE_PRESETS: SignaturePreset[] = [
  {
    id: "classic",
    label: "Classic",
    template:
      '<div style="font-family:Arial,Helvetica,sans-serif;color:#1f2937;line-height:1.6"><div style="font-size:20px;font-weight:700;letter-spacing:-0.02em">{name}</div>{{#profession}}<div style="margin-top:2px;font-size:12px;color:#6b7280">{profession}</div>{{/profession}}<div style="margin-top:11px;font-size:12px">{{#email}}{email_link}{{/email}}{{#phone}} <span style="color:#9ca3af">·</span> {phone_link}{{/phone}}{{#location}}<br>{location}{{/location}}{{#website}}<br>{website_link}{{/website}}</div>{{#socials}}<div style="margin-top:10px">{socials}</div>{{/socials}}</div>'
  },
  {
    id: "studio",
    label: "Studio",
    template:
      '<div style="font-family:Arial,Helvetica,sans-serif;color:#1f2937;line-height:1.6;border-left:4px solid #6c5ce7;padding:1px 0 1px 16px"><div style="font-size:18px;font-weight:700">{name}</div>{{#profession}}<div style="margin-top:2px;font-size:10px;color:#6c5ce7;text-transform:uppercase;letter-spacing:0.13em">{profession}</div>{{/profession}}<div style="margin-top:9px;font-size:12px">{{#email}}{email_link}{{/email}}{{#phone}} <span style="color:#9ca3af">·</span> {phone_link}{{/phone}}{{#location}}<br>{location}{{/location}}{{#website}}<br>{website_link}{{/website}}</div>{{#socials}}<div style="margin-top:9px">{socials}</div>{{/socials}}</div>'
  },
  {
    id: "split",
    label: "Split",
    template:
      '<table role="presentation" cellspacing="0" cellpadding="0" style="font-family:Arial,Helvetica,sans-serif;color:#1f2937;border-collapse:collapse"><tr><td style="vertical-align:top;padding-right:22px;border-right:1px solid #e5e7eb"><div style="font-size:18px;font-weight:700;line-height:1.35">{name}</div>{{#profession}}<div style="margin-top:3px;font-size:12px;color:#6b7280">{profession}</div>{{/profession}}</td><td style="vertical-align:top;padding-left:22px;font-size:12px;line-height:1.75">{{#email}}<div>{email_link}</div>{{/email}}{{#phone}}<div>{phone_link}</div>{{/phone}}{{#location}}<div>{location}</div>{{/location}}{{#website}}<div>{website_link}</div>{{/website}}</td></tr>{{#socials}}<tr><td colspan="2" style="padding-top:10px">{socials}</td></tr>{{/socials}}</table>'
  },
  {
    id: "card",
    label: "Card",
    template:
      '<div style="display:inline-block;min-width:250px;font-family:Arial,Helvetica,sans-serif;color:#1f2937;line-height:1.6;border:1px solid #e5e7eb;border-radius:14px;padding:16px 18px;background:#fafafa"><div style="font-size:18px;font-weight:700">{name}</div>{{#profession}}<div style="margin-top:2px;font-size:12px;color:#6b7280">{profession}</div>{{/profession}}<div style="margin-top:11px;font-size:12px">{{#email}}<div>{email_link}</div>{{/email}}{{#phone}}<div>{phone_link}</div>{{/phone}}{{#location}}<div>{location}</div>{{/location}}{{#website}}<div>{website_link}</div>{{/website}}</div>{{#socials}}<div style="margin-top:10px">{socials}</div>{{/socials}}</div>'
  },
  {
    id: "aura",
    label: "Aura",
    template:
      '<div style="display:inline-block;min-width:250px;font-family:Arial,Helvetica,sans-serif;color:#4b5563;line-height:1.6;border-radius:16px;padding:16px 19px;background:#f0edff;box-shadow:inset 0 0 0 1px #ddd6fe"><div style="font-size:18px;font-weight:700;color:#3730a3">{name}</div>{{#profession}}<div style="margin-top:2px;font-size:11px;color:#6d5bd0;text-transform:uppercase;letter-spacing:0.08em">{profession}</div>{{/profession}}<div style="margin-top:9px;font-size:12px">{{#email}}{email_link}{{/email}}{{#phone}} <span style="color:#9ca3af">·</span> {phone_link}{{/phone}}{{#location}}<br>{location}{{/location}}{{#website}}<br>{website_link}{{/website}}</div>{{#socials}}<div style="margin-top:9px">{socials}</div>{{/socials}}</div>'
  },
  {
    id: "compact",
    label: "Compact",
    template:
      '<div style="font-family:Arial,Helvetica,sans-serif;color:#374151;font-size:12px;line-height:1.7"><strong style="font-size:14px;color:#1f2937">{name}</strong>{{#profession}} <span style="color:#9ca3af">·</span> {profession}{{/profession}}{{#email}} <span style="color:#9ca3af">·</span> {email_link}{{/email}}{{#phone}} <span style="color:#9ca3af">·</span> {phone_link}{{/phone}}{{#location}} <span style="color:#9ca3af">·</span> {location}{{/location}}{{#website}} <span style="color:#9ca3af">·</span> {website_link}{{/website}}{{#socials}}<div style="margin-top:7px">{socials}</div>{{/socials}}</div>'
  },
  {
    id: "banner",
    label: "Banner",
    template:
      '<table role="presentation" cellspacing="0" cellpadding="0" style="font-family:Arial,Helvetica,sans-serif;color:#1f2937;border-collapse:collapse;min-width:260px"><tr><td style="padding:14px 18px;border-radius:10px 10px 0 0;background:#111827;color:#ffffff"><div style="font-size:18px;font-weight:700">{name}</div>{{#profession}}<div style="margin-top:2px;font-size:11px;color:#c7d2fe;text-transform:uppercase;letter-spacing:0.1em">{profession}</div>{{/profession}}</td></tr><tr><td style="padding:12px 18px;border:1px solid #e5e7eb;border-top:0;border-radius:0 0 10px 10px;background:#ffffff;font-size:12px;line-height:1.65">{{#email}}<div>{email_link}</div>{{/email}}{{#phone}}<div>{phone_link}</div>{{/phone}}{{#location}}<div>{location}</div>{{/location}}{{#website}}<div>{website_link}</div>{{/website}}{{#socials}}<div style="margin-top:8px">{socials}</div>{{/socials}}</td></tr></table>'
  },
  {
    id: "minimal",
    label: "Minimal",
    template:
      '<div style="font-family:Arial,Helvetica,sans-serif;color:#111827;line-height:1.55"><div style="font-size:11px;font-weight:700;letter-spacing:0.16em;text-transform:uppercase">{name}</div>{{#profession}}<div style="margin-top:3px;font-size:11px;color:#6b7280">{profession}</div>{{/profession}}<div style="width:36px;height:1px;margin:11px 0;background:#111827"></div><div style="font-size:11px;color:#4b5563">{{#email}}{email_link}{{/email}}{{#phone}} <span style="color:#9ca3af">·</span> {phone_link}{{/phone}}{{#location}}<br>{location}{{/location}}{{#website}}<br>{website_link}{{/website}}</div>{{#socials}}<div style="margin-top:9px">{socials}</div>{{/socials}}</div>'
  },
  {
    id: "editorial",
    label: "Editorial",
    template:
      '<div style="font-family:Georgia,\'Times New Roman\',serif;color:#1f2937;line-height:1.55"><div style="font-size:24px;font-weight:400;letter-spacing:-0.02em">{name}</div>{{#profession}}<div style="margin-top:2px;font-size:10px;color:#6b7280;text-transform:uppercase;letter-spacing:0.14em">{profession}</div>{{/profession}}<div style="height:1px;margin:12px 0;background:#d1d5db"></div><div style="font-family:Arial,Helvetica,sans-serif;font-size:11px;line-height:1.65;color:#4b5563">{{#email}}{email_link}{{/email}}{{#phone}} <span style="color:#9ca3af">·</span> {phone_link}{{/phone}}{{#location}}<br>{location}{{/location}}{{#website}}<br>{website_link}{{/website}}</div>{{#socials}}<div style="margin-top:9px">{socials}</div>{{/socials}}</div>'
  },
  {
    id: "terminal",
    label: "Terminal",
    template:
      '<div style="font-family:\'Courier New\',Courier,monospace;color:#111827;font-size:12px;line-height:1.7"><div style="font-size:14px;font-weight:700">&gt; {name}</div>{{#profession}}<div style="color:#6b7280">// {profession}</div>{{/profession}}<div style="margin-top:7px">{{#email}}{email_link}{{/email}}{{#phone}} <span style="color:#9ca3af">/</span> {phone_link}{{/phone}}{{#location}}<br>{location}{{/location}}{{#website}}<br>{website_link}{{/website}}</div>{{#socials}}<div style="margin-top:8px">{socials}</div>{{/socials}}</div>'
  }
];

function escapeSignatureText(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function safeSignatureUrl(value: string): string {
  const url = value.trim();
  if (/^https?:\/\//i.test(url) || /^mailto:/i.test(url) || /^tel:/i.test(url)) {
    return escapeSignatureText(url);
  }
  return "";
}

function socialIcon(label: string, url: string): string {
  const href = safeSignatureUrl(url);
  if (!href) return "";
  const network = socialNetworkFor(label, url);
  const name = escapeSignatureText(network?.label || label.trim() || "Link");
  const src = network?.src ?? socialIconSrc(label, url);
  return `<a href="${href}" title="${name}" style="display:inline-block;margin:0 6px 4px 0;text-decoration:none"><img src="${src}" alt="${name}" width="24" height="24" style="display:block;width:24px;height:24px;border:0;border-radius:50%"></a>`;
}

export function resolveSignature(
  template: string,
  account: SignatureAccount
): string {
  const fields = account.signature_fields;
  const nameRaw = account.display_name.trim() || account.email.trim();
  const emailRaw = account.email.trim();
  const firstNameRaw = nameRaw.split(/\s+/)[0] || emailRaw;
  const professionRaw = fields?.profession.trim() ?? "";
  const phoneRaw = fields?.phone.trim() ?? "";
  const locationRaw = fields?.location.trim() ?? "";
  const websiteRaw = fields?.website.trim() ?? "";
  const socialsRaw = (fields?.socials ?? []).filter((social) =>
    Boolean(safeSignatureUrl(social.url))
  );

  const name = escapeSignatureText(nameRaw);
  const email = escapeSignatureText(emailRaw);
  const firstName = escapeSignatureText(firstNameRaw);
  const profession = escapeSignatureText(professionRaw);
  const phone = escapeSignatureText(phoneRaw);
  const location = escapeSignatureText(locationRaw);
  const website = escapeSignatureText(websiteRaw);
  const websiteHref = safeSignatureUrl(
    /^(https?:\/\/|mailto:|tel:)/i.test(websiteRaw)
      ? websiteRaw
      : websiteRaw
        ? `https://${websiteRaw}`
        : ""
  );
  const emailHref = safeSignatureUrl(emailRaw ? `mailto:${emailRaw}` : "");
  const phoneHref = safeSignatureUrl(phoneRaw ? `tel:${phoneRaw}` : "");
  const socials = socialsRaw
    .map((social) => socialIcon(social.label, social.url))
    .filter(Boolean)
    .join("");
  const values: Record<string, string> = {
    name,
    display_name: name,
    first_name: firstName,
    email,
    email_link: emailHref ? `<a href="${emailHref}" style="color:#4f46e5;text-decoration:none">${email}</a>` : email,
    profession,
    phone,
    phone_link: phoneHref ? `<a href="${phoneHref}" style="color:#4f46e5;text-decoration:none">${phone}</a>` : phone,
    location,
    website,
    website_link: websiteHref ? `<a href="${websiteHref}" style="color:#4f46e5;text-decoration:none">${website}</a>` : website,
    socials
  };
  const present: Record<string, boolean> = {
    name: Boolean(nameRaw),
    display_name: Boolean(nameRaw),
    first_name: Boolean(firstNameRaw),
    email: Boolean(emailRaw),
    email_link: Boolean(emailRaw),
    profession: Boolean(professionRaw),
    phone: Boolean(phoneRaw),
    phone_link: Boolean(phoneRaw),
    location: Boolean(locationRaw),
    website: Boolean(websiteRaw),
    website_link: Boolean(websiteRaw),
    socials: socialsRaw.length > 0
  };

  let output = template.replace(
    /\{\{#(\w+)\}\}([\s\S]*?)\{\{\/\1\}\}/gi,
    (_match, key: string, body: string) =>
      present[key.toLowerCase()] ? body : ""
  );
  output = output.replace(/\{(\w+)\}/g, (_match, key: string) =>
    values[key.toLowerCase()] ?? ""
  );
  return output;
}

export function sanitizeSignatureImages(html: string): string {
  return html.replace(/<img\b[^>]*>/gi, (tag) => {
    const match = tag.match(/\bsrc\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))/i);
    const src = (match?.[1] ?? match?.[2] ?? match?.[3] ?? "").trim();
    if (!/^(data:image\/|blob:)/i.test(src)) return "";
    return tag.replace(/\s+srcset\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)/gi, "");
  });
}

export function signatureTemplateHtml(template: string): string {
  const trimmed = template.trim();
  if (!trimmed) return "";
  return /<\/?(?:a|b|br|div|em|h[1-6]|hr|i|img|li|ol|p|span|strong|table|tbody|td|tfoot|th|thead|tr|u|ul)\b[^>]*>/i.test(
    trimmed
  )
    ? trimmed
    : textToHtml(trimmed);
}

export function signatureHtml(
  template: string,
  account: SignatureAccount
): string {
  const resolved = resolveSignature(template, account).trim();
  if (!resolved) return "";
  const html = signatureTemplateHtml(resolved);
  return `<div data-auramail-signature="true">${html}</div>`;
}

export function appendSignature(html: string, signature: string): string {
  if (!signature) return html;
  return html.trim() ? `${html}<br><br>${signature}` : signature;
}
