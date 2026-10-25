// @ts-expect-error Vitest runs in Node, but the app tsconfig has no Node types.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync("src/app.css", "utf8");

function ruleBody(selector: string): string {
  const start = css.indexOf(`${selector} {`);
  if (start < 0) return "";
  const open = css.indexOf("{", start);
  const close = css.indexOf("}", open);
  return css.slice(open + 1, close);
}

describe("high contrast theme roles", () => {
  it("forces maximum-contrast surfaces and text", () => {
    const body = ruleBody('html[data-high-contrast="true"]');

    expect(body).toContain("--bg: #000");
    expect(body).toContain("--bg-raised: #000");
    expect(body).toContain("--text: #fff");
    expect(body).toContain("--text-strong: #fff");
    expect(body).toContain("--muted: #fff");
    expect(body).toContain("--border: #fff");
    expect(body).not.toContain("--text: var(--base06)");
    expect(css).toContain("background-image: none !important");
    expect(css).toContain("filter: none !important");
  });

  it("removes every animation and transition", () => {
    const start = css.indexOf('html[data-reduce-motion="true"] *');
    const end = css.indexOf("}", start);
    const body = start < 0 || end < 0 ? "" : css.slice(start, end + 1);

    expect(body).toContain("animation: none !important");
    expect(body).toContain("transition: none !important");
    expect(body).toContain("scroll-behavior: auto !important");
  });
});
