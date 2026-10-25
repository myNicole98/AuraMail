const SAFE_SCHEMES = new Set(["http:", "https:", "mailto:"]);

export type ThemeColors = {
  background: string;
  backgroundRaised: string;
  text: string;
  link: string;
};

export function bodyCsp(): string {
  return "default-src 'none'; script-src 'none'; img-src data:; style-src 'unsafe-inline'; style-src-elem 'unsafe-inline'; style-src-attr 'unsafe-inline'; font-src data:; base-uri 'none'; form-action 'none';";
}

function skipString(css: string, start: number): number {
  const quote = css[start];
  let index = start + 1;
  while (index < css.length) {
    if (css[index] === "\\") index += 2;
    else if (css[index] === quote) return index + 1;
    else index += 1;
  }
  return index;
}

function skipComment(css: string, start: number): number {
  const end = css.indexOf("*/", start + 2);
  return end === -1 ? css.length : end + 2;
}

function blockEnd(css: string, open: number): number {
  let depth = 0;
  let index = open;
  while (index < css.length) {
    const char = css[index];
    if (char === '"' || char === "'") {
      index = skipString(css, index);
      continue;
    }
    if (char === "/" && css[index + 1] === "*") {
      index = skipComment(css, index);
      continue;
    }
    if (char === "{") depth += 1;
    else if (char === "}") {
      depth -= 1;
      if (depth === 0) return index + 1;
    }
    index += 1;
  }
  return css.length;
}

function stripDarkRules(css: string): string {
  let output = "";
  let index = 0;
  while (index < css.length) {
    const start = index;
    while (index < css.length && /\s/.test(css[index])) index += 1;
    if (index >= css.length) {
      output += css.slice(start);
      break;
    }
    let probe = index;
    let brace = -1;
    while (probe < css.length) {
      const char = css[probe];
      if (char === '"' || char === "'") {
        probe = skipString(css, probe);
        continue;
      }
      if (char === "/" && css[probe + 1] === "*") {
        probe = skipComment(css, probe);
        continue;
      }
      if (char === "{") {
        brace = probe;
        break;
      }
      if (char === ";" || char === "}") break;
      probe += 1;
    }
    if (brace === -1) {
      const semicolon = css.indexOf(";", index);
      if (semicolon === -1) {
        output += css.slice(start);
        break;
      }
      output += css.slice(start, semicolon + 1);
      index = semicolon + 1;
      continue;
    }
    const prelude = css.slice(index, brace).trim().toLowerCase();
    const end = blockEnd(css, brace);
    const block = css.slice(start, end);
    const documentSelectors = prelude.split(",").map((selector) => selector.trim());
    const documentRule =
      documentSelectors.length > 0 &&
      documentSelectors.every(
        (selector) => selector === "body" || selector === "html" || selector === "html body"
      );
    if (
      documentRule ||
      (prelude.startsWith("@media") &&
        prelude.includes("prefers-color-scheme") &&
        prelude.includes("dark")) ||
      prelude.includes("[data-ogsc]") ||
      prelude.includes("[data-ogsb]")
    ) {
      index = end;
      continue;
    }
    output += block;
    index = end;
  }
  return output;
}

export function stripEmailDarkCss(html: string): string {
  return html.replace(/<style>([\s\S]*?)<\/style>/gi, (_match, css: string) => {
    return `<style>${stripDarkRules(css)}</style>`;
  });
}

export const QUOTE_SELECTOR =
  ".protonmail_quote, .gmail_quote, blockquote[type='cite'], #divRplyFwdMsg, #appendonsend, .yahoo_quoted";

export function hasQuotedContent(html: string): boolean {
  if (!html) return false;
  const parsed = new DOMParser().parseFromString(html, "text/html");
  return Boolean(parsed.querySelector(QUOTE_SELECTOR));
}

function normalizeSubject(value: string): string {
  return value
    .replace(/\s+/g, " ")
    .replace(/\s*(?:\.{3,}|…)\s*$/u, "")
    .trim()
    .toLowerCase();
}

function decodeText(value: string): string {
  if (typeof DOMParser === "undefined") {
    return value.replace(/&nbsp;/gi, " ");
  }
  return new DOMParser().parseFromString(value, "text/html").body.textContent ?? value;
}

export function stripDuplicatePreheader(html: string, subject: string): string {
  const leading = /^\s*([^<]+)\s*/.exec(html);
  if (!leading) return html;
  const text = normalizeSubject(decodeText(leading[1]));
  const expected = normalizeSubject(subject);
  if (!text || !expected) return html;
  const shortest = Math.min(text.length, expected.length);
  const same = text === expected;
  const related =
    shortest >= 16 &&
    (text.includes(expected) || expected.includes(text));
  if (!same && !related) return html;
  return html.slice(leading[0].length);
}

type Rgba = { r: number; g: number; b: number; a: number };

function parseCssColor(value: string | null | undefined): Rgba | null {
  if (!value) return null;
  const color = value.trim().toLowerCase();
  if (!color || ["inherit", "currentcolor", "initial", "unset"].includes(color)) {
    return null;
  }
  if (color === "transparent") return { r: 0, g: 0, b: 0, a: 0 };
  if (color === "white") return { r: 255, g: 255, b: 255, a: 1 };
  if (color === "black") return { r: 0, g: 0, b: 0, a: 1 };

  const hex = /^#([0-9a-f]{3,8})$/i.exec(color);
  if (hex) {
    const raw = hex[1];
    const expand = (part: string) =>
      part.length === 1 ? Number.parseInt(part + part, 16) : Number.parseInt(part, 16);
    if (raw.length === 3 || raw.length === 4) {
      return {
        r: expand(raw[0]),
        g: expand(raw[1]),
        b: expand(raw[2]),
        a: raw.length === 4 ? expand(raw[3]) / 255 : 1
      };
    }
    if (raw.length === 6 || raw.length === 8) {
      return {
        r: expand(raw.slice(0, 2)),
        g: expand(raw.slice(2, 4)),
        b: expand(raw.slice(4, 6)),
        a: raw.length === 8 ? expand(raw.slice(6, 8)) / 255 : 1
      };
    }
  }

  const rgb = /^rgba?\(([^)]+)\)$/i.exec(color);
  if (!rgb) return null;
  const parts = rgb[1].split(/[\s,\/]+/).filter(Boolean);
  if (parts.length < 3) return null;
  const channel = (part: string) => {
    const number = Number.parseFloat(part);
    if (!Number.isFinite(number)) return 0;
    return part.endsWith("%") ? (number / 100) * 255 : number;
  };
  const alpha = parts[3] === undefined ? 1 : channel(parts[3]);
  return {
    r: Math.max(0, Math.min(255, channel(parts[0]))),
    g: Math.max(0, Math.min(255, channel(parts[1]))),
    b: Math.max(0, Math.min(255, channel(parts[2]))),
    a: Math.max(0, Math.min(1, alpha))
  };
}

function luminance({ r, g, b }: Rgba): number {
  const channel = (value: number) => {
    const normalized = value / 255;
    return normalized <= 0.03928
      ? normalized / 12.92
      : ((normalized + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function isDarkColor(value: string): boolean {
  const color = parseCssColor(value);
  return color !== null && color.a > 0.1 && luminance(color) < 0.5;
}

function elementBackground(element: HTMLElement): string | null {
  const style = element.style;
  return (
    style.backgroundColor ||
    style.getPropertyValue("background-color") ||
    style.getPropertyValue("background") ||
    element.getAttribute("bgcolor")
  );
}

function hasBackgroundImage(element: HTMLElement): boolean {
  const image = element.style.backgroundImage;
  return Boolean(image && image !== "none");
}

function darkenEmailContent(content: string, colors: ThemeColors): string {
  if (typeof document === "undefined" || isDarkColor(colors.text)) {
    return content;
  }

  const template = document.createElement("template");
  template.innerHTML = content;

  const walk = (parent: ParentNode, surface: "light" | "dark") => {
    for (const node of Array.from(parent.children)) {
      const element = node as HTMLElement;
      let nextSurface = surface;
      const backgroundValue = elementBackground(element);
      const background = parseCssColor(backgroundValue);
      const hasImage = hasBackgroundImage(element);
      if (background && background.a > 0.1 && !hasImage) {
        if (luminance(background) > 0.6) {
          element.style.backgroundColor = colors.backgroundRaised;
          element.removeAttribute("bgcolor");
          nextSurface = "dark";
        } else {
          nextSurface = "dark";
        }
      } else if (hasImage && background) {
        nextSurface = luminance(background) > 0.6 ? "light" : surface;
      }

      const colorValue =
        element.style.color ||
        element.style.getPropertyValue("color") ||
        element.getAttribute("color");
      const color = parseCssColor(colorValue);
      if (color && color.a > 0.1 && nextSurface === "dark" && luminance(color) < 0.5) {
        element.style.color =
          element.tagName.toLowerCase() === "a" ? colors.link : colors.text;
        element.removeAttribute("color");
      }

      walk(element, nextSurface);
    }
  };

  walk(template.content, "dark");
  return template.innerHTML;
}

function lazyDecodeImages(html: string): string {
  const lazy = html.replace(/<img\b(?![^>]*\bloading\s*=)/gi, '<img loading="lazy"');
  return lazy.replace(/<img\b(?![^>]*\bdecoding\s*=)/gi, '<img decoding="async"');
}

export function bodyDocument(
  html: string,
  colors: ThemeColors,
  darkEmail = false,
  allowImages = true,
  subject = ""
): string {
  let content = darkEmail ? html : stripEmailDarkCss(html);
  content = stripDuplicatePreheader(content, subject);
  if (!allowImages) content = blockImageSources(content);
  if (darkEmail) content = darkenEmailContent(content, colors);
  content = lazyDecodeImages(content);
  const background = darkEmail ? "transparent" : "#ffffff";
  const text = darkEmail ? colors.text : "#111111";
  const link = darkEmail ? colors.link : "#0b57d0";
  return [
    "<!doctype html><html><head><meta charset=\"utf-8\">",
    `<meta http-equiv="Content-Security-Policy" content="${bodyCsp()}">`,
    "<style>",
    `html{color-scheme:${darkEmail ? "dark" : "light"};background:transparent}`,
    `body{margin:0;background:${background}${darkEmail ? "!important" : ""};color:${text};font:14px/1.6 system-ui,-apple-system,sans-serif;overflow-wrap:anywhere}`,
    "img{max-width:100%;height:auto}",
    "*,*::before,*::after{will-change:auto!important}",
    "pre{white-space:pre-wrap}",
    `${QUOTE_SELECTOR}{display:none!important}`,
    `body.auramail-show-quotes :is(${QUOTE_SELECTOR}){display:block!important}`,
    `a{color:${link}}`,
    "</style></head><body>",
    content,
    "</body></html>"
  ].join("");
}

export function hasRemoteImages(html: string | null | undefined): boolean {
  if (!html) return false;
  const document = new DOMParser().parseFromString(html, "text/html");
  return Array.from(document.querySelectorAll("img[src]")).some((image) => {
    const src = image.getAttribute("src")?.trim().toLowerCase() ?? "";
    return src.startsWith("http://") || src.startsWith("https://");
  });
}

export function hasImages(html: string | null | undefined): boolean {
  if (!html) return false;
  const document = new DOMParser().parseFromString(html, "text/html");
  return Array.from(document.querySelectorAll("img[src]")).some((image) => {
    return Boolean(image.getAttribute("src")?.trim());
  });
}

const TRANSPARENT_PIXEL =
  "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==";

export function blockImageSources(html: string): string {
  return html.replace(/<img\b[^>]*>/gi, (tag) =>
    tag.replace(
      /\bsrc\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)/i,
      `src="${TRANSPARENT_PIXEL}"`
    )
  );
}

export function isSafeExternalUrl(value: string): boolean {
  try {
    return SAFE_SCHEMES.has(new URL(value).protocol);
  } catch {
    return false;
  }
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const rounded = unit === 0 ? Math.round(value) : Number(value.toFixed(1));
  return `${rounded} ${units[unit]}`;
}
