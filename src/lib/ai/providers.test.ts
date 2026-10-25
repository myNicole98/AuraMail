import { describe, expect, it } from "vitest";
import {
  modelIconUrl,
  providerById,
  providerForUrl,
  providerIconUrls,
  providers
} from "./providers";

describe("AI providers", () => {
  it("detects providers from their base URL", () => {
    expect(providerForUrl("https://api.openai.com/v1")).toBe("openai");
    expect(providerForUrl("https://api.deepseek.com/v1")).toBe("deepseek");
    expect(providerForUrl("https://openrouter.ai/api/v1")).toBe("openrouter");
    expect(providerForUrl("http://localhost:11434/v1")).toBe("ollama");
    expect(providerForUrl("http://127.0.0.1:1234/v1")).toBe("lmstudio");
    expect(providerForUrl("")).toBe("openai");
    expect(providerForUrl("https://example.com/v1")).toBe("custom");
  });

  it("exposes provider defaults and key placeholders", () => {
    expect(providerById("deepseek").baseUrl).toBe("https://api.deepseek.com/v1");
    expect(providerById("deepseek").keyPlaceholder).toBe("dk-xxxxxxxxxxxx");
    expect(providerById("openai").fixed).toBe(true);
    expect(providerById("ollama").fixed).toBe(false);
    expect(providerById("ollama").requiresKey).toBe(false);
    expect(providerById("custom").fixed).toBe(false);
    expect(providers.find((provider) => provider.id === "custom")).toBeDefined();
  });

  it("falls back to the light provider icon when no dark variant exists", () => {
    expect(providerIconUrls("openai").dark).not.toBeNull();
    expect(providerIconUrls("deepseek").light).not.toBeNull();
    expect(providerIconUrls("deepseek").dark).toBeNull();
    expect(providerIconUrls("google").light).not.toBeNull();
    expect(providerIconUrls("google").dark).toBeNull();
  });

  it("matches model icons by model name", () => {
    const icons = [
      modelIconUrl("gpt-4o-mini"),
      modelIconUrl("deepseek-chat"),
      modelIconUrl("claude-sonnet-4"),
      modelIconUrl("gemini-2.0-flash"),
      modelIconUrl("llama-3.3-70b-versatile"),
      modelIconUrl("qwen2.5-coder")
    ];
    expect(icons.every(Boolean)).toBe(true);
    expect(new Set(icons).size).toBe(icons.length);
    expect(modelIconUrl("unknown-model")).toBeNull();
  });
});
