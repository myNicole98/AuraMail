import { describe, expect, it } from "vitest";
import {
  bodyDocument,
  bodyCsp,
  formatBytes,
  hasImages,
  hasQuotedContent,
  hasRemoteImages,
  isSafeExternalUrl,
  stripEmailDarkCss
} from "./reader";

describe("formatBytes", () => {
  it("uses byte and kilobyte units", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
  });

  it("uses megabytes for large attachments", () => {
    expect(formatBytes(25 * 1024 * 1024)).toBe("25 MB");
  });
});

describe("hasImages", () => {
  it("detects remote and embedded image sources", () => {
    expect(hasImages('<img src="https://example.com/a.png">')).toBe(true);
    expect(hasImages('<img src="data:image/png;base64,AAAA">')).toBe(true);
    expect(hasImages("<p>No images</p>")).toBe(false);
  });
});

describe("hasRemoteImages", () => {
  it("detects absolute http and https image sources", () => {
    expect(hasRemoteImages('<img src="https://example.com/a.png">')).toBe(true);
    expect(hasRemoteImages("<img src='http://example.com/a.png'>")).toBe(true);
  });

  it("ignores data images, non-src attributes, and missing bodies", () => {
    expect(hasRemoteImages('<img src="data:image/png;base64,AAAA">')).toBe(false);
    expect(hasRemoteImages('<img data-src="https://example.com/a.png">')).toBe(false);
    expect(hasRemoteImages(null)).toBe(false);
  });
});

const colors = {
  background: "#101010",
  backgroundRaised: "#1a1a1a",
  text: "#e0e0e0",
  link: "#4c7dff"
};

function parseDocument(html: string): Document {
  return new DOMParser().parseFromString(html, "text/html");
}

describe("bodyCsp", () => {
  it("blocks remote images and form navigation", () => {
    expect(bodyCsp()).toContain("img-src data:;");
    expect(bodyCsp()).not.toContain("http:");
    expect(bodyCsp()).toContain("base-uri 'none';");
    expect(bodyCsp()).toContain("form-action 'none';");
    expect(bodyCsp()).toContain("default-src 'none';");
    expect(bodyCsp()).toContain("script-src 'none';");
  });
});

describe("bodyDocument", () => {
  it("wraps the body in a CSP-protected document", () => {
    const document = bodyDocument("<p>Hello</p>", colors);
    expect(document).toContain("Content-Security-Policy");
    expect(document).toContain("<p>Hello</p>");
    expect(document).toContain("img-src data:;");
    expect(document).toContain("background:#ffffff");
    expect(document).toContain("will-change:auto!important");
  });

  it("lazily decodes offscreen images and preserves explicit choices", () => {
    const document = bodyDocument(
      '<p>Hello</p><img src="data:image/png;base64,AAAA"><img loading="eager" decoding="sync" src="https://example.com/a.png">',
      colors
    );
    const body = parseDocument(document).body;
    const images = [...body.querySelectorAll("img")];
    expect(images).toHaveLength(2);
    expect(images[0].getAttribute("loading")).toBe("lazy");
    expect(images[0].getAttribute("decoding")).toBe("async");
    expect(images[1].getAttribute("loading")).toBe("eager");
    expect(images[1].getAttribute("decoding")).toBe("sync");
    expect(document.match(/\bloading=/g)).toHaveLength(2);
    expect(document.match(/\bdecoding=/g)).toHaveLength(2);
  });

  it("detects quoted-thread containers", () => {
    expect(hasQuotedContent('<div class="protonmail_quote">Earlier</div>')).toBe(true);
    expect(hasQuotedContent('<div class="gmail_quote">Earlier</div>')).toBe(true);
    expect(hasQuotedContent("<p>No quote here</p>")).toBe(false);
  });

  it("collapses known quoted-thread containers", () => {
    const document = bodyDocument(
      '<div class="protonmail_quote">Earlier</div>',
      colors
    );
    expect(document).toContain(".protonmail_quote");
    expect(document).toContain("display:none!important");
    expect(document).toContain("auramail-show-quotes");
  });

  it("uses a light canvas by default so authored light emails stay readable", () => {
    const document = bodyDocument("<p>Hello</p>", colors);
    expect(document).toContain("background:#ffffff");
    expect(document).toContain("color:#111111");
    expect(document).toContain("a{color:#0b57d0}");
  });

  it("recognises off-white email cards", () => {
    const document = bodyDocument(
      '<div style="background-color:#F5F5F5"><p>Hello</p></div>',
      colors
    );
    expect(document).toContain("background:#ffffff");
    expect(document).toContain("color:#111111");
  });

  it("leaves author colours and button text alone", () => {
    const document = bodyDocument(
      '<a style="color:#ffffff;background-color:#0b57d0">Controlla</a>',
      colors
    );
    expect(document).not.toContain("color:inherit!important");
    expect(document).toContain("color:#ffffff");
  });

  it("strips a leading preheader that repeats the subject", () => {
    const html =
      '\n    Ricevuta del tuo pagamento a favore di Hangzhou DeepSeek Ar...\n    <h4 style="display:none">Nicole</h4><p>Body</p>';
    const document = bodyDocument(
      html,
      colors,
      false,
      true,
      "Ricevuta del tuo pagamento a favore di Hangzhou DeepSeek Ar..."
    );
    expect(document).not.toContain("Ricevuta del tuo pagamento");
    expect(document).toContain("<p>Body</p>");
  });

  it("matches an encoded, truncated preheader against the subject", () => {
    const document = bodyDocument(
      '\n Deghi SpA: 134,40&nbsp;&euro;&nbsp;EUR \n <p>Body</p>',
      colors,
      false,
      true,
      "Deghi SpA: 134,40 € EUR"
    );
    expect(document).not.toContain("Deghi SpA");
    expect(document).toContain("<p>Body</p>");
  });

  it("strips a shortened preheader contained in the subject", () => {
    const document = bodyDocument(
      "\n    Epic Games Receipt\n    <p>Body</p>",
      colors,
      false,
      true,
      "Your Epic Games Receipt F2412171214190511"
    );
    expect(document).not.toContain("Epic Games Receipt");
    expect(document).toContain("<p>Body</p>");
  });

  it("keeps leading content that is not the subject", () => {
    const document = bodyDocument(
      "Gentile Nicole,<br>ecco il preventivo.",
      colors,
      false,
      true,
      "Preventivo richiesto"
    );
    expect(document).toContain("Gentile Nicole");
  });

  it("uses a transparent canvas in email dark mode so the theme shows through", () => {
    const document = bodyDocument("<p>Hello</p>", colors, true);
    expect(document).toContain("background:transparent");
    expect(document).toContain(`color:${colors.text}`);
    expect(document).toContain(`a{color:${colors.link}}`);
  });

  it("darkens even when the theme background is a CSS colour expression", () => {
    const document = parseDocument(
      bodyDocument(
        '<p style="color:#000000">Copy</p>',
        {
          ...colors,
          background: "color-mix(in srgb, #202020 70%, #101010)"
        },
        true
      )
    );
    expect(document.querySelector("p")?.style.color).toBe("rgb(224, 224, 224)");
  });

  it("recolours dark author text for a dark canvas", () => {
    const document = parseDocument(
      bodyDocument(
        '<div><p style="color:rgba(0, 0, 0, 0.87)">Copy</p></div>',
        colors,
        true
      )
    );
    expect(document.querySelector("p")?.style.color).toBe("rgb(224, 224, 224)");
  });

  it("keeps white button text on an authored dark background", () => {
    const document = parseDocument(
      bodyDocument(
        '<a style="color:#ffffff;background-color:#0b57d0">Controlla</a>',
        colors,
        true
      )
    );
    expect(document.querySelector("a")?.style.color).toBe("rgb(255, 255, 255)");
    expect(document.querySelector("a")?.style.backgroundColor).toBe(
      "rgb(11, 87, 208)"
    );
  });

  it("turns light author blocks into a raised dark surface", () => {
    const document = parseDocument(
      bodyDocument(
        '<div id="authored" style="background-color:#F5F5F5"><span style="color:rgba(0, 0, 0, 0.54)">Copy</span></div>',
        colors,
        true
      )
    );
    expect(
      (document.querySelector("#authored") as HTMLElement | null)?.style
        .backgroundColor
    ).toBe("rgb(26, 26, 26)");
    expect(document.querySelector("span")?.style.color).toBe(
      "rgb(224, 224, 224)"
    );
  });

  it("replaces blocked image sources with a transparent pixel", () => {
    const document = bodyDocument(
      '<img src="https://example.com/logo.png" width="120">',
      colors,
      false,
      false
    );
    expect(document).not.toContain("https://example.com/logo.png");
    expect(document).toContain("data:image/gif;base64");
    expect(document).toContain('width="120"');
  });

  it("keeps image sources when images are allowed", () => {
    const document = bodyDocument(
      '<img src="https://example.com/logo.png">',
      colors,
      false,
      true
    );
    expect(document).toContain("https://example.com/logo.png");
  });
});

describe("stripEmailDarkCss", () => {
  const html = `<style>.light{color:#111}@media (prefers-color-scheme:dark){body{background:#181a1a}.footer{background:#303333}}[data-ogsc] .x{color:#fff}</style><p>Hello</p>`;

  it("removes dark-mode media queries and Outlook dark rules", () => {
    const clean = stripEmailDarkCss(html);
    expect(clean).toContain(".light");
    expect(clean).not.toContain("prefers-color-scheme");
    expect(clean).not.toContain("data-ogsc");
    expect(clean).toContain("<p>Hello</p>");
  });

  it("removes email body and html chrome so the reader background stays themed", () => {
    const clean = stripEmailDarkCss(
      `<style>body{background:#fff!important;color:#000}html{background:#fff}.content{background:#fff}</style><div class="content">Hi</div>`
    );
    expect(clean).not.toMatch(/\bbody\s*\{/);
    expect(clean).not.toMatch(/\bhtml\s*\{/);
    expect(clean).toContain(".content");
  });

  it("keeps dark-mode CSS when email dark mode is enabled", () => {
    const document = bodyDocument(html, colors, true);
    expect(document).toContain("prefers-color-scheme:dark");
    expect(document).toContain("background:#303333");
  });

  it("strips dark-mode CSS by default", () => {
    const document = bodyDocument(html, colors);
    expect(document).not.toContain("prefers-color-scheme");
    expect(document).not.toContain("data-ogsc");
  });
});

describe("isSafeExternalUrl", () => {
  it("allows web and mail links", () => {
    expect(isSafeExternalUrl("https://example.com")).toBe(true);
    expect(isSafeExternalUrl("http://example.com")).toBe(true);
    expect(isSafeExternalUrl("mailto:a@example.com")).toBe(true);
  });

  it("rejects script, data, file, and relative links", () => {
    expect(isSafeExternalUrl("javascript:alert(1)")).toBe(false);
    expect(isSafeExternalUrl("data:text/html,x")).toBe(false);
    expect(isSafeExternalUrl("file:///etc/passwd")).toBe(false);
    expect(isSafeExternalUrl("/local")).toBe(false);
  });
});
